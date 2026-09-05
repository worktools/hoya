//! Minimal v1 runtimes: no filesystem, environment, network, or process APIs.
use crate::protocol::Request;
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use wasmtime::{Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

type Failure = (String, String);
fn failure(code: &str, message: impl ToString) -> Failure {
    (code.into(), message.to_string())
}
#[derive(Default)]
struct Logs {
    entries: Vec<Value>,
    bytes: usize,
    exceeded: bool,
}
impl Logs {
    fn push(&mut self, entry: Value, max: usize) -> bool {
        let size = serde_json::to_vec(&entry)
            .map(|v| v.len() + 1)
            .unwrap_or(max + 1);
        if self.bytes + size > max {
            self.exceeded = true;
            return false;
        }
        self.bytes += size;
        self.entries.push(entry);
        true
    }
}

pub fn javascript(req: &Request, logs: &mut Vec<Value>) -> Result<Value, Failure> {
    use rquickjs::{Context, Function, Runtime, Value as JsValue};
    let runtime = Runtime::new().map_err(|e| failure("RUNTIME_ERROR", e))?;
    let deadline = Instant::now() + Duration::from_millis(req.limits.timeout_ms);
    runtime.set_memory_limit(req.limits.memory_mb * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context = Context::full(&runtime).map_err(|e| failure("RUNTIME_ERROR", e))?;
    let captured = Arc::new(Mutex::new(Logs::default()));
    let result = context.with(|ctx| {
        let operation = || -> rquickjs::Result<Value> {
            let buffer=captured.clone(); let max=req.limits.max_log_bytes;
            let log=Function::new(ctx.clone(), move |encoded: String| {
                let entry=serde_json::from_str(&encoded).unwrap_or(json!({"level":"info","message":encoded}));
                buffer.lock().unwrap().push(entry,max)
            })?;
            // Capture the wrapper before guest code can change the builtins it uses.
            let wrapper: Function = ctx.eval(r#"(function(log) {
                const stringify = JSON.stringify, text = String, date = Date.now;
                return Object.freeze({
                    log(level, message, fields = null) {
                        if (!log(stringify({level: ['debug','info','warn','error'].includes(level) ? level : 'info', message:text(message), fields, at:date()}))) throw new Error('LOG_LIMIT');
                    }, now: date
                });
            })"#)?;
            let guest_ctx: JsValue=wrapper.call((log,))?;
            let input=ctx.json_parse(req.input.to_string())?;
            ctx.eval::<(),_>(req.code.as_str())?;
            let main: Function=ctx.globals().get("main")?;
            let value: JsValue=main.call((input,guest_ctx))?;
            let value=if let Some(promise)=value.as_promise() {
                // Bounded drain including already-resolved promises; no asynchronous I/O.
                let mut answer=None;
                for _ in 0..10000 {
                    if Instant::now()>=deadline { return Err(rquickjs::Error::WouldBlock); }
                    if let Some(result)=promise.result::<JsValue>() { answer=Some(result?); break; }
                    if !ctx.execute_pending_job() { break; }
                }
                answer.ok_or(rquickjs::Error::WouldBlock)?
            } else { value };
            let encoded=ctx.json_stringify(value)?.ok_or(rquickjs::Error::FromJs { from:"undefined",to:"JSON",message:Some("main must return a JSON value".into()) })?.to_string()?;
            if encoded.len()>req.limits.max_result_bytes { return Err(rquickjs::Error::FromJs { from:"result",to:"JSON",message:Some("RESULT_LIMIT".into()) }); }
            serde_json::from_str(&encoded).map_err(|_|rquickjs::Error::FromJs { from:"result",to:"JSON",message:None })
        };
        operation().map_err(|e| {
            if Instant::now()>=deadline { return failure("EXECUTION_TIMEOUT","JavaScript exceeded its budget"); }
            if matches!(e,rquickjs::Error::WouldBlock) { return failure("PROMISE_UNSETTLED","Promise did not settle within the supported microtask budget; async I/O is unavailable"); }
            let message=if e.is_exception() {
                let caught=ctx.catch();
                caught.as_object().and_then(|v|v.get::<_,String>("message").ok()).or_else(||caught.as_string().and_then(|v|v.to_string().ok())).unwrap_or_else(||e.to_string())
            } else { e.to_string() };
            failure(if message.contains("RESULT_LIMIT") {"RESULT_LIMIT"} else {"USER_CODE_ERROR"},message)
        })
    });
    let mut captured = captured.lock().unwrap();
    *logs = std::mem::take(&mut captured.entries);
    if captured.exceeded {
        return Err(failure("LOG_LIMIT", "Log byte budget exceeded"));
    }
    result
}

struct WasmState {
    input: Vec<u8>,
    logs: Logs,
    max_logs: usize,
    limits: StoreLimits,
}
fn memory(caller: &mut Caller<'_, WasmState>) -> anyhow::Result<wasmtime::Memory> {
    caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow::anyhow!("memory export required"))
}
pub fn wasm(req: &Request, bytes: &[u8], logs: &mut Vec<Value>) -> Result<Value, Failure> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.max_wasm_stack(512 * 1024);
    let engine = Engine::new(&config).map_err(|e| failure("RUNTIME_ERROR", e))?;
    let module = Module::from_binary(&engine, bytes).map_err(|e| failure("INVALID_WASM", e))?;
    let mut linker = Linker::<WasmState>::new(&engine);
    let imports = (|| -> anyhow::Result<()> {
        linker.func_wrap(
            "env",
            "get_input",
            |mut caller: Caller<'_, WasmState>, ptr: u32, capacity: u32| -> anyhow::Result<u32> {
                let len = caller.data().input.len();
                if capacity == 0 {
                    return Ok(len as u32);
                }
                anyhow::ensure!(capacity as usize >= len, "input buffer too small");
                let input = caller.data().input.clone();
                memory(&mut caller)?.write(&mut caller, ptr as usize, &input)?;
                Ok(len as u32)
            },
        )?;
        linker.func_wrap("env","log",|mut caller:Caller<'_,WasmState>,ptr:u32,len:u32| -> anyhow::Result<()> {
            let mem=memory(&mut caller)?;
            let max=caller.data().max_logs;
            anyhow::ensure!(len as usize<=max,"LOG_LIMIT");
            let end=(ptr as usize).checked_add(len as usize).ok_or_else(||anyhow::anyhow!("invalid log pointer"))?;
            let bytes=mem.data(&caller).get(ptr as usize..end).ok_or_else(||anyhow::anyhow!("invalid log pointer"))?;
            let message=std::str::from_utf8(bytes)?;
            let entry=json!({"level":"info","message":message,"fields":null,"at":chrono::Utc::now().timestamp_millis()});
            anyhow::ensure!(caller.data_mut().logs.push(entry,max),"LOG_LIMIT"); Ok(())
        })?;
        linker.func_wrap("env", "now", || chrono::Utc::now().timestamp_millis())?;
        Ok(())
    })();
    imports.map_err(|e| failure("RUNTIME_ERROR", e))?;
    let mut store = Store::new(
        &engine,
        WasmState {
            input: serde_json::to_vec(&req.input).unwrap(),
            logs: Logs::default(),
            max_logs: req.limits.max_log_bytes,
            limits: StoreLimitsBuilder::new()
                .memory_size(req.limits.memory_mb * 1024 * 1024)
                .table_elements(10000)
                .instances(1)
                .memories(1)
                .tables(1)
                .trap_on_grow_failure(true)
                .build(),
        },
    );
    store.limiter(|s| &mut s.limits);
    store
        .set_fuel(10_000_000)
        .map_err(|e| failure("RUNTIME_ERROR", e))?;
    let execute = (|| -> anyhow::Result<Value> {
        let instance = linker.instantiate(&mut store, &module)?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("memory export required"))?;
        let main = instance.get_typed_func::<(), i32>(&mut store, "hoya_main")?;
        let pointer = main.call(&mut store, ())?;
        anyhow::ensure!(pointer >= 0, "invalid result pointer");
        let data = memory.data(&store);
        let start = pointer as usize;
        let end = start
            .saturating_add(req.limits.max_result_bytes + 1)
            .min(data.len());
        let result = data
            .get(start..end)
            .ok_or_else(|| anyhow::anyhow!("invalid result pointer"))?;
        let length = result
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| anyhow::anyhow!("RESULT_LIMIT or missing NUL terminator"))?;
        Ok(serde_json::from_slice(&result[..length])?)
    })();
    *logs = std::mem::take(&mut store.data_mut().logs.entries);
    if store.data().logs.exceeded {
        return Err(failure("LOG_LIMIT", "Log byte budget exceeded"));
    }
    execute.map_err(|e| {
        let msg = format!("{e:#}");
        let code = if msg.contains("fuel") {
            "FUEL_EXHAUSTED"
        } else if msg.contains("RESULT_LIMIT") {
            "RESULT_LIMIT"
        } else if msg.contains("LOG_LIMIT") {
            "LOG_LIMIT"
        } else {
            "WASM_EXECUTION_ERROR"
        };
        failure(code, msg)
    })
}
