//! Bounded execution of CPU-heavy guest runtimes.
//!
//! QuickJS and Wasmtime execute synchronously.  Running them directly in an
//! Axum handler occupies Tokio's I/O workers, so all guest work is sent to the
//! blocking pool and protected by a small, process-wide admission gate.

use crate::error::AppError;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

const DEFAULT_MAX_CONCURRENT_EXECUTIONS: usize = 4;
const MAX_CONCURRENT_EXECUTIONS: usize = 64;

static EXECUTION_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn execution_slots() -> Arc<Semaphore> {
    EXECUTION_SLOTS
        .get_or_init(|| {
            let configured = std::env::var("HOYA_MAX_CONCURRENT_EXECUTIONS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| (1..=MAX_CONCURRENT_EXECUTIONS).contains(value))
                .unwrap_or(DEFAULT_MAX_CONCURRENT_EXECUTIONS);
            Arc::new(Semaphore::new(configured))
        })
        .clone()
}

/// Run guest code outside Tokio's core workers. A permit is deliberately held
/// inside the blocking closure, which bounds both queued guest executions and
/// the number of blocking threads that can be consumed by sandbox work.
pub async fn run_blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    let permit = execution_slots()
        .acquire_owned()
        .await
        .map_err(|_| AppError::Internal("execution admission gate closed".to_string()))?;

    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|error| AppError::Internal(format!("sandbox worker failed: {error}")))?
}
