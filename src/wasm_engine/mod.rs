mod ffis;

use crate::error::{AppError, ErrorInfo, ExecuteResponse, ExecutionMetadata};
use crate::wasm_engine::ffis as wasm_ffis;
use axum::Json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, Once};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wasmtime::{
    Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, Trap,
};

/// Context kept per Wasm instance. The client and engine are shared; all guest
/// memory and captured output remain per invocation.
pub struct WasmCtx {
    pub reqwest_client: reqwest::Client,
    pub memory: Option<Memory>,
    pub stdout: Arc<Mutex<String>>,
    pub stderr: Arc<Mutex<String>>,
    pub input_json: Option<String>,
    pub datasource_json: Option<String>,
    pub limits: StoreLimits,
}

/// Instruction budget for one guest call. This is intentionally independent of
/// the host's queueing and network timeouts.
const DEFAULT_FUEL_LIMIT: u64 = 100_000;
/// Wasmtime linear-memory cap per instance.
const DEFAULT_MAX_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const MAX_RESULT_BYTES: usize = 1024 * 1024;
const MODULE_CACHE_CAPACITY: usize = 64;
const EPOCH_TICK: Duration = Duration::from_millis(10);
const WASM_WALL_CLOCK_TIMEOUT: Duration = Duration::from_secs(5);
const WASM_TIMEOUT_TICKS: u64 =
    (WASM_WALL_CLOCK_TIMEOUT.as_millis() / EPOCH_TICK.as_millis()) as u64;

static ENGINE: LazyLock<Engine> = LazyLock::new(|| {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.max_wasm_stack(500_000);
    Engine::new(&config).expect("valid static Wasmtime configuration")
});

struct CachedModule {
    module: Module,
    last_used: u64,
}

#[derive(Default)]
struct ModuleCache {
    clock: u64,
    modules: HashMap<[u8; 32], CachedModule>,
}

static MODULE_CACHE: LazyLock<Mutex<ModuleCache>> =
    LazyLock::new(|| Mutex::new(ModuleCache::default()));
static EPOCH_CLOCK_STARTED: Once = Once::new();

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        // Do not inherit host proxy discovery for sandbox egress. Besides
        // keeping the network boundary explicit, this avoids platform proxy
        // discovery during ordinary guest executions that never call fetch.
        .no_proxy()
        .pool_max_idle_per_host(8)
        .build()
        .expect("valid static reqwest client configuration")
});

fn start_epoch_clock() {
    EPOCH_CLOCK_STARTED.call_once(|| {
        std::thread::Builder::new()
            .name("hoya-wasm-epoch".to_string())
            .spawn(|| loop {
                std::thread::sleep(EPOCH_TICK);
                ENGINE.increment_epoch();
            })
            .expect("start Wasmtime epoch clock");
    });
}

fn cached_module(wasm: &[u8]) -> Result<Module, AppError> {
    let key: [u8; 32] = Sha256::digest(wasm).into();
    if let Ok(mut cache) = MODULE_CACHE.lock() {
        cache.clock += 1;
        let clock = cache.clock;
        if let Some(cached) = cache.modules.get_mut(&key) {
            cached.last_used = clock;
            return Ok(cached.module.clone());
        }
    }

    let compiled = Module::from_binary(&ENGINE, wasm)
        .map_err(|error| AppError::Internal(format!("failed to compile WASM module: {error}")))?;
    let mut cache = MODULE_CACHE
        .lock()
        .map_err(|_| AppError::Internal("WASM module cache lock poisoned".to_string()))?;
    cache.clock += 1;
    let clock = cache.clock;
    if let Some(cached) = cache.modules.get_mut(&key) {
        cached.last_used = clock;
        return Ok(cached.module.clone());
    }
    if cache.modules.len() >= MODULE_CACHE_CAPACITY {
        if let Some(oldest) = cache
            .modules
            .iter()
            .min_by_key(|(_, cached)| cached.last_used)
            .map(|(key, _)| *key)
        {
            cache.modules.remove(&oldest);
        }
    }
    cache.modules.insert(
        key,
        CachedModule {
            module: compiled.clone(),
            last_used: clock,
        },
    );
    Ok(compiled)
}

fn timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => chrono::DateTime::<chrono::Utc>::from_timestamp(
            duration.as_secs() as i64,
            duration.subsec_nanos(),
        )
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339(),
        Err(_) => chrono::Utc::now().to_rfc3339(),
    }
}

fn new_store(
    input_json: Option<String>,
    datasource_json: Option<String>,
) -> Result<Store<WasmCtx>, AppError> {
    start_epoch_clock();
    let ctx = WasmCtx {
        reqwest_client: HTTP_CLIENT.clone(),
        memory: None,
        stdout: Arc::new(Mutex::new(String::new())),
        stderr: Arc::new(Mutex::new(String::new())),
        input_json,
        datasource_json,
        limits: StoreLimitsBuilder::new()
            .memory_size(DEFAULT_MAX_MEMORY_BYTES)
            .trap_on_grow_failure(true)
            .build(),
    };
    let mut store = Store::new(&ENGINE, ctx);
    store.limiter(|ctx| &mut ctx.limits);
    store
        .set_fuel(DEFAULT_FUEL_LIMIT)
        .map_err(|error| AppError::Internal(format!("failed to set WASM fuel: {error}")))?;
    // Fuel measures guest instructions; epoch interruption also provides a
    // wall-clock backstop when calibration or future host calls make fuel an
    // insufficient proxy for elapsed execution time.
    store.set_epoch_deadline(WASM_TIMEOUT_TICKS);
    store.epoch_deadline_trap();
    Ok(store)
}

fn instantiate(store: &mut Store<WasmCtx>, wasm: &[u8]) -> Result<wasmtime::Instance, AppError> {
    let module = cached_module(wasm)?;
    let mut linker = Linker::new(&ENGINE);
    wasm_ffis::register_linker_functions(&mut linker).map_err(|error| {
        AppError::Internal(format!("failed to register WASM host functions: {error}"))
    })?;
    let instance = linker
        .instantiate(&mut *store, &module)
        .map_err(AppError::Wasmtime)?;
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| AppError::Internal("WASM module must export memory".to_string()))?;
    store.data_mut().memory = Some(memory);
    Ok(instance)
}

fn captured_output(store: &Store<WasmCtx>) -> (String, String) {
    let stdout = store
        .data()
        .stdout
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let stderr = store
        .data()
        .stderr
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    (stdout, stderr)
}

fn read_nul_terminated(
    memory: Memory,
    store: &Store<WasmCtx>,
    pointer: i32,
) -> Result<String, AppError> {
    if pointer < 0 {
        return Err(AppError::Internal(
            "WASM main returned a negative result pointer".to_string(),
        ));
    }
    let data = memory.data(store);
    let start = pointer as usize;
    let end_limit = start
        .checked_add(MAX_RESULT_BYTES)
        .ok_or_else(|| AppError::Internal("WASM result pointer overflow".to_string()))?
        .min(data.len());
    let bytes = data
        .get(start..end_limit)
        .ok_or_else(|| AppError::Internal("WASM result pointer out of bounds".to_string()))?;
    let end = bytes.iter().position(|byte| *byte == 0).ok_or_else(|| {
        AppError::Internal(format!(
            "WASM result exceeds {MAX_RESULT_BYTES} byte limit or is not NUL-terminated"
        ))
    })?;
    String::from_utf8(bytes[..end].to_vec())
        .map_err(|_| AppError::Internal("WASM result is not valid UTF-8".to_string()))
}

fn metadata(start: Instant, resource_size: usize) -> ExecutionMetadata {
    ExecutionMetadata {
        execution_time: start.elapsed().as_millis() as u64,
        code_type: "webassembly".to_string(),
        timestamp: timestamp(),
        resource_size,
    }
}

fn failed_response(
    store: &Store<WasmCtx>,
    start: Instant,
    resource_size: usize,
    code: &str,
    message: String,
) -> Json<ExecuteResponse> {
    let (stdout, stderr) = captured_output(store);
    Json(ExecuteResponse {
        status: "failed".to_string(),
        output: None,
        stdout: Some(stdout),
        stderr: Some(stderr),
        error: Some(ErrorInfo {
            code: code.to_string(),
            message,
            details: None,
        }),
        metadata: metadata(start, resource_size),
    })
}

fn execution_error_code(error: &anyhow::Error) -> &'static str {
    let trap = error.chain().find_map(|cause| cause.downcast_ref::<Trap>());
    if matches!(trap, Some(Trap::OutOfFuel)) {
        return "WASM_FUEL_EXHAUSTED";
    }
    if matches!(trap, Some(Trap::Interrupt)) {
        return "WASM_TIMEOUT";
    }
    if matches!(trap, Some(Trap::MemoryOutOfBounds))
        || error.chain().any(|cause| {
            let message = cause.to_string();
            message.contains("memory") && (message.contains("grow") || message.contains("limit"))
        })
    {
        return "WASM_MEMORY_LIMIT";
    }
    "WASM_EXECUTION_ERROR"
}

/// Execute a legacy standalone module. This compatibility API is deliberately
/// explicit: it requires `_start` and never reports a module as successful
/// merely because instantiation succeeded.
pub fn execute_wasm(downloaded_code: bytes::Bytes) -> Result<Json<ExecuteResponse>, AppError> {
    let start = Instant::now();
    let resource_size = downloaded_code.len();
    let mut store = new_store(None, None)?;
    let instance = instantiate(&mut store, &downloaded_code)?;
    let start_function = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .map_err(|_| AppError::Internal("legacy WASM module must export _start()".to_string()))?;

    match start_function.call(&mut store, ()) {
        Ok(()) => {
            let (stdout, stderr) = captured_output(&store);
            Ok(Json(ExecuteResponse {
                status: "success".to_string(),
                output: Some("WASM module executed (_start)".to_string()),
                stdout: Some(stdout),
                stderr: Some(stderr),
                error: None,
                metadata: metadata(start, resource_size),
            }))
        }
        Err(error) => Ok(failed_response(
            &store,
            start,
            resource_size,
            execution_error_code(&error),
            error.to_string(),
        )),
    }
}

/// Execute Hosta's WASM ABI: `memory` plus `main() -> i32`, where the result
/// pointer addresses a NUL-terminated JSON envelope. `main` is mandatory; a
/// missing entry point is a failed request rather than a false success.
pub fn execute_wasm_with_input(
    downloaded_code: bytes::Bytes,
    input_json: Option<String>,
    datasource_json: Option<String>,
) -> Result<Json<ExecuteResponse>, AppError> {
    let start = Instant::now();
    let resource_size = downloaded_code.len();
    let mut store = new_store(input_json, datasource_json)?;
    let instance = instantiate(&mut store, &downloaded_code)?;
    let main = instance
        .get_typed_func::<(), i32>(&mut store, "main")
        .map_err(|_| {
            AppError::Internal("Hosta WASM module must export main() -> i32".to_string())
        })?;

    let pointer = match main.call(&mut store, ()) {
        Ok(pointer) => pointer,
        Err(error) => {
            return Ok(failed_response(
                &store,
                start,
                resource_size,
                execution_error_code(&error),
                error.to_string(),
            ));
        }
    };

    let memory = store
        .data()
        .memory
        .ok_or_else(|| AppError::Internal("WASM memory not initialized".to_string()))?;
    match read_nul_terminated(memory, &store, pointer) {
        Ok(output) => {
            let (stdout, stderr) = captured_output(&store);
            Ok(Json(ExecuteResponse {
                status: "success".to_string(),
                output: Some(output),
                stdout: Some(stdout),
                stderr: Some(stderr),
                error: None,
                metadata: metadata(start, resource_size),
            }))
        }
        Err(error) => Ok(failed_response(
            &store,
            start,
            resource_size,
            "WASM_INVALID_RESULT",
            error.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wat(source: &str) -> bytes::Bytes {
        bytes::Bytes::from(wat::parse_str(source).expect("valid WAT fixture"))
    }

    #[test]
    fn hosta_abi_returns_the_main_result() {
        let response = execute_wasm_with_input(
            wat(r#"(module
                (memory (export "memory") 1)
                (data (i32.const 16) "{\"ok\":true,\"data\":\"hello\"}\00")
                (func (export "main") (result i32) (i32.const 16))
            )"#),
            Some("{}".to_string()),
            None,
        )
        .expect("execution response")
        .0;

        assert_eq!(response.status, "success");
        assert_eq!(
            response.output.as_deref(),
            Some(r#"{"ok":true,"data":"hello"}"#)
        );
    }

    #[test]
    fn hosta_abi_rejects_a_missing_main_entrypoint() {
        let error = execute_wasm_with_input(
            wat(r#"(module (memory (export "memory") 1) (func (export "_start")))"#),
            None,
            None,
        )
        .expect_err("missing main must not be reported as success");

        assert!(error.to_string().contains("must export main"));
    }

    #[test]
    fn fuel_exhaustion_is_a_failed_execution_response() {
        let response = execute_wasm_with_input(
            wat(r#"(module
                (memory (export "memory") 1)
                (func (export "main") (result i32)
                    (loop br 0)
                    (i32.const 0))
            )"#),
            None,
            None,
        )
        .expect("fuel exhaustion is reported in an execution response")
        .0;

        assert_eq!(response.status, "failed");
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("WASM_FUEL_EXHAUSTED")
        );
    }

    #[test]
    fn memory_growth_past_32_mib_is_rejected() {
        let response = execute_wasm_with_input(
            wat(r#"(module
                (memory (export "memory") 1)
                (func (export "main") (result i32)
                    (drop (memory.grow (i32.const 512)))
                    (i32.const 0))
            )"#),
            None,
            None,
        )
        .expect("memory failure is reported in an execution response")
        .0;

        assert_eq!(response.status, "failed");
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("WASM_MEMORY_LIMIT")
        );
    }
}
