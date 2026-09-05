//! Versioned execution boundary. v1 is intentionally network-free.
use axum::{extract::rejection::JsonRejection, http::StatusCode, Json};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::{Arc, OnceLock},
    time::Instant,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};

pub const MAX_BODY: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub timeout_ms: u64,
    pub memory_mb: usize,
    pub max_log_bytes: usize,
    pub max_result_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout_ms: 3000,
            memory_mb: 32,
            max_log_bytes: 65536,
            max_result_bytes: 1048576,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    #[serde(default)]
    pub network: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: String,
    pub run_id: String,
    pub runtime: String,
    pub code: String,
    pub artifact_sha256: String,
    pub input: Value,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub capabilities: Capabilities,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub protocol_version: String,
    pub run_id: String,
    pub artifact_sha256: String,
    pub status: String,
    pub result: Value,
    pub logs: Vec<Value>,
    pub metrics: Value,
    pub error: Option<ExecutionError>,
}
impl Response {
    pub fn new(req: &Request) -> Self {
        Self {
            protocol_version: "1".into(),
            run_id: req.run_id.clone(),
            artifact_sha256: req.artifact_sha256.clone(),
            status: "succeeded".into(),
            result: Value::Null,
            logs: vec![],
            metrics: json!({"durationMs":0}),
            error: None,
        }
    }
    pub fn fail(mut self, status: &str, code: &str, message: &str) -> Self {
        self.status = status.into();
        self.result = Value::Null;
        self.error = Some(ExecutionError {
            code: code.into(),
            message: message.chars().take(2000).collect(),
            retryable: matches!(status, "internal_error") || code == "OVERLOADED",
        });
        self
    }
}
impl Request {
    pub fn artifact(&self) -> Result<Vec<u8>, &'static str> {
        if self.protocol_version != "1" {
            return Err("UNSUPPORTED_PROTOCOL");
        }
        if self.run_id.is_empty() || self.run_id.len() > 128 {
            return Err("INVALID_RUN_ID");
        }
        if !self.capabilities.network.is_empty() {
            return Err("UNSUPPORTED_CAPABILITY");
        }
        let l = &self.limits;
        if !(10..=10000).contains(&l.timeout_ms)
            || !(8..=64).contains(&l.memory_mb)
            || !(1..=65536).contains(&l.max_log_bytes)
            || !(1..=1048576).contains(&l.max_result_bytes)
        {
            return Err("INVALID_LIMITS");
        }
        let bytes = match self.runtime.as_str() {
            "javascript" if self.code.len() <= 131072 => self.code.as_bytes().to_vec(),
            "wasm" => base64::engine::general_purpose::STANDARD
                .decode(&self.code)
                .map_err(|_| "INVALID_ARTIFACT")?,
            _ => return Err("UNSUPPORTED_RUNTIME_OR_SOURCE_SIZE"),
        };
        if bytes.is_empty() || bytes.len() > 1048576 {
            return Err("INVALID_ARTIFACT_SIZE");
        }
        if format!("{:x}", Sha256::digest(&bytes)) != self.artifact_sha256 {
            return Err("ARTIFACT_HASH_MISMATCH");
        }
        Ok(bytes)
    }
}

pub async fn capabilities() -> Json<Value> {
    Json(json!({
        "service":"hoya", "version":env!("CARGO_PKG_VERSION"), "protocolVersions":["1"],
        "runtimes":["javascript","wasm"], "wasmAbi":"hoya-json-v1", "javascriptEntry":"script-main",
        "network":false, "asyncIo":false, "isolation":"process-per-execution",
        "limits":Limits::default(), "maxRequestBytes":MAX_BODY,
        "limitsScope":"guest heap/linear memory; use container limits for total process RSS"
    }))
}

static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
pub async fn execute(payload: Result<Json<Request>, JsonRejection>) -> (StatusCode, Json<Value>) {
    let req = match payload {
        Ok(Json(r)) => r,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"error":{"code":"INVALID_REQUEST","message":"Body must conform to Hoya execution v1","retryable":false}}),
                ),
            )
        }
    };
    let response = Response::new(&req);
    if let Err(code) = req.artifact() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!(response.fail(
                "rejected",
                code,
                "Execution request rejected"
            ))),
        );
    }
    let slots = SLOTS.get_or_init(|| Arc::new(Semaphore::new(4)));
    let _permit = match slots.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!(response.fail(
                    "rejected",
                    "OVERLOADED",
                    "All execution slots are busy; retry later"
                ))),
            )
        }
    };
    let start = Instant::now();
    let deadline = std::time::Duration::from_millis(req.limits.timeout_ms);
    // Dropping this future on timeout/cancellation drops the owned child and kills it.
    let execution = async {
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--execution-worker")
            .env_clear()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&serde_json::to_vec(&req)?).await?;
        drop(stdin);
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .unwrap()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(std::io::Error::other("worker response too large"));
        }
        if !child.wait().await?.success() {
            return Err(std::io::Error::other("worker exited"));
        }
        let parsed: Response = serde_json::from_slice(&bytes)?;
        if parsed.run_id != req.run_id
            || parsed.artifact_sha256 != req.artifact_sha256
            || parsed.protocol_version != "1"
        {
            return Err(std::io::Error::other("worker correlation mismatch"));
        }
        Ok::<Response, std::io::Error>(parsed)
    };
    let mut result = match tokio::time::timeout(deadline, execution).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => response.fail("internal_error", "WORKER_FAILED", "Execution worker failed"),
        Err(_) => response.fail(
            "timed_out",
            "EXECUTION_TIMEOUT",
            "Execution exceeded its wall-clock budget",
        ),
    };
    result.metrics = json!({"durationMs":start.elapsed().as_millis() as u64});
    (StatusCode::OK, Json(json!(result)))
}

pub fn worker() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = vec![];
    std::io::stdin()
        .take((MAX_BODY + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BODY {
        return Err("worker input too large".into());
    }
    let req: Request = serde_json::from_slice(&bytes)?;
    let response = run(&req);
    std::io::stdout().write_all(&serde_json::to_vec(&response)?)?;
    Ok(())
}
pub fn run(req: &Request) -> Response {
    let mut response = Response::new(req);
    let bytes = match req.artifact() {
        Ok(b) => b,
        Err(code) => return response.fail("rejected", code, "Execution request rejected"),
    };
    let result = if req.runtime == "javascript" {
        crate::runtime_v1::javascript(req, &mut response.logs)
    } else {
        crate::runtime_v1::wasm(req, &bytes, &mut response.logs)
    };
    match result {
        Ok(value) => {
            response.result = value;
            response
        }
        Err((code, message)) => {
            let status = if code == "EXECUTION_TIMEOUT" {
                "timed_out"
            } else {
                "failed"
            };
            response.fail(status, &code, &message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(runtime: &str, code: &[u8]) -> Request {
        Request {
            protocol_version: "1".into(),
            run_id: "test-run".into(),
            runtime: runtime.into(),
            code: if runtime == "wasm" {
                base64::engine::general_purpose::STANDARD.encode(code)
            } else {
                String::from_utf8(code.to_vec()).unwrap()
            },
            artifact_sha256: format!("{:x}", Sha256::digest(code)),
            input: json!({"text":"`${data}`\\\n中文", "nested":[null,true]}),
            limits: Limits::default(),
            capabilities: Capabilities::default(),
        }
    }
    #[test]
    fn javascript_roundtrips_values_and_captures_structured_logs() {
        let req=request("javascript",b"async function main(input, ctx) { ctx.log('info', 'hello', {value: 7}); return await Promise.resolve(input); }");
        let response = run(&req);
        assert_eq!(response.status, "succeeded");
        assert_eq!(response.result, req.input);
        assert_eq!(response.logs[0]["fields"], json!({"value":7}));
    }
    #[test]
    fn wasm_uses_same_json_result_contract() {
        let code=wat::parse_str(r#"(module (import "env" "get_input" (func $input (param i32 i32) (result i32))) (memory (export "memory") 2) (func (export "hoya_main") (result i32) i32.const 1024 i32.const 65535 call $input drop i32.const 1024))"#).unwrap();
        let req = request("wasm", &code);
        let response = run(&req);
        assert_eq!(response.status, "succeeded");
        assert_eq!(response.result, req.input);
    }
    #[test]
    fn rejects_mismatch_and_unsupported_capabilities_before_execution() {
        let mut req = request("javascript", b"function main() { return 1; }");
        req.artifact_sha256 = "0".repeat(64);
        assert_eq!(run(&req).error.unwrap().code, "ARTIFACT_HASH_MISMATCH");
        req.capabilities.network.push("example.com".into());
        assert_eq!(run(&req).error.unwrap().code, "UNSUPPORTED_CAPABILITY");
    }
    #[test]
    fn invalid_results_missing_main_and_unsettled_promises_are_failures() {
        for source in [
            "const x=1;",
            "function main() {}",
            "function main() { const a={}; a.a=a; return a; }",
            "async function main() { throw new Error('failed'); }",
            "function main() { return new Promise(() => {}); }",
        ] {
            let response = run(&request("javascript", source.as_bytes()));
            assert_ne!(response.status, "succeeded", "{source}");
            assert!(response.error.is_some());
        }
    }
    #[test]
    fn log_and_result_budgets_are_enforced_even_when_guest_catches_log_error() {
        let mut req=request("javascript",b"function main(input,ctx) { try {ctx.log('info','x'.repeat(1000));} catch(e) {} return 1; }");
        req.limits.max_log_bytes = 100;
        assert_eq!(run(&req).error.unwrap().code, "LOG_LIMIT");
        let mut req = request(
            "javascript",
            b"function main() { return 'x'.repeat(1000); }",
        );
        req.limits.max_result_bytes = 100;
        assert_eq!(run(&req).error.unwrap().code, "RESULT_LIMIT");
    }
    #[test]
    fn runtimes_have_no_network_imports_and_fresh_contexts() {
        let req=request("javascript",b"function main(){ return [typeof fetch, typeof process, typeof globalThis.previous]; }");
        assert_eq!(
            run(&req).result,
            json!(["undefined", "undefined", "undefined"])
        );
        let _ = run(&request(
            "javascript",
            b"function main(){globalThis.previous=1; return 1;}",
        ));
        assert_eq!(
            run(&req).result,
            json!(["undefined", "undefined", "undefined"])
        );
        let code=wat::parse_str(r#"(module (import "env" "fetch" (func)) (memory (export "memory") 1) (func (export "hoya_main") (result i32) i32.const 0))"#).unwrap();
        assert_eq!(run(&request("wasm", &code)).status, "failed");
    }
}
