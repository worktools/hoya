//! # WebAssembly FFI Functions
//! 
//! This module provides Foreign Function Interface (FFI) functions for WebAssembly modules.
//! It registers functions that can be called from WebAssembly code, such as logging,
//! time utilities, and HTTP fetch functionality.

use anyhow::{anyhow, Result as AnyhowResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Caller, Linker};

use crate::WasmCtx;

/// Data structures for Wasm fetch communication (JSON)
/// 
/// These are duplicates from main.rs. Consider moving them to a shared module
/// or passing them as part of WasmCtx if they are only used by these FFI functions.
#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchOptions {
    /// URL to send the request to
    url: String,
    /// HTTP method (e.g., "GET", "POST")
    method: String,
    /// HTTP headers
    headers: HashMap<String, String>,
    /// Optional request body as string, could be base64 for binary data
    body: Option<String>,
}

/// HTTP response data for WebAssembly modules
#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchResponse {
    /// HTTP status code
    status: u16,
    /// Response headers
    headers: HashMap<String, String>,
    /// Response body as text or base64-encoded binary
    body: String,
    /// Optional error information
    error: Option<WasmFetchError>,
}

/// Error information for HTTP requests
#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchError {
    /// Error code identifier
    code: String,
    /// Error message
    message: String,
}

/// Register WebAssembly FFI functions with the linker
///
/// This function registers all FFI functions that can be called from WebAssembly code,
/// including logging, time utilities, and HTTP fetch functionality.
pub fn register_linker_functions(linker: &mut Linker<WasmCtx>) -> AnyhowResult<()> {
    // Register app_log function for WebAssembly logging
    linker.func_wrap(
        "env",
        "app_log",
        |caller: Caller<'_, WasmCtx>,
         level_ptr: u32,
         level_len: u32,
         msg_ptr: u32,
         msg_len: u32|
         -> AnyhowResult<()> {
            let memory = caller
                .data()
                .memory
                .ok_or_else(|| anyhow!("app_log: memory not initialized in WasmCtx"))?;
            let level_bytes = memory
                .data(&caller)
                .get(level_ptr as usize..(level_ptr + level_len) as usize)
                .ok_or_else(|| anyhow!("app_log: level pointer/length out of bounds"))?;
            let level_str = std::str::from_utf8(level_bytes)
                .map_err(|_| anyhow!("app_log: level not valid UTF-8"))?;
            let msg_bytes = memory
                .data(&caller)
                .get(msg_ptr as usize..(msg_ptr + msg_len) as usize)
                .ok_or_else(|| anyhow!("app_log: message pointer/length out of bounds"))?;
            let msg_str = std::str::from_utf8(msg_bytes)
                .map_err(|_| anyhow!("app_log: message not valid UTF-8"))?;
            println!("[WASM LOG - {}]: {}", level_str.to_uppercase(), msg_str);
            Ok(())
        },
    )?;

    // Register get_unixtime function for system time access
    linker.func_wrap(
        "env",
        "get_unixtime",
        |_caller: Caller<'_, WasmCtx>| -> AnyhowResult<u64> {
            match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(n) => Ok(n.as_secs()),
                Err(_) => Err(anyhow!("get_unixtime: Failed to get system time")),
            }
        },
    )?;

    // Register fetch function for HTTP requests
    linker.func_wrap(
        "env",
        "fetch",
        |mut caller: Caller<'_, WasmCtx>,
         options_ptr: u32,
         options_len: u32,
         resp_buf_ptr: u32,
         resp_buf_max_len: u32|
         -> AnyhowResult<i32> {
            let memory = caller
                .data()
                .memory
                .ok_or_else(|| anyhow!("fetch: memory not initialized in WasmCtx"))?;

            let options_bytes_vec: Vec<u8> = memory
                .data(&caller)
                .get(options_ptr as usize..(options_ptr + options_len) as usize)
                .ok_or_else(|| anyhow!("fetch: options pointer/length out of bounds"))?
                .to_vec();

            let fetch_options: WasmFetchOptions = serde_json::from_slice(&options_bytes_vec)
                .map_err(|e| anyhow!("fetch: failed to deserialize options JSON: {}", e))?;

            let http_method = reqwest::Method::from_bytes(fetch_options.method.as_bytes())
                .map_err(|_| {
                    anyhow!(
                        "fetch: invalid HTTP method string: {}",
                        fetch_options.method
                    )
                })?;

            let mut http_headers = reqwest::header::HeaderMap::new();
            for (key, value) in fetch_options.headers {
                let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())
                    .map_err(|_| anyhow!("fetch: invalid header name {}", key))?;
                let header_value = reqwest::header::HeaderValue::from_str(&value)
                    .map_err(|_| anyhow!("fetch: invalid header value for {}", key))?;
                http_headers.insert(header_name, header_value);
            }

            // Use the client from WasmCtx instead of creating a new one
            let client = &caller.data().reqwest_client;
            let mut request_builder = client
                .request(http_method, &fetch_options.url)
                .headers(http_headers);

            if let Some(body_str) = fetch_options.body {
                request_builder = request_builder.body(body_str);
            }

            // reqwest::blocking::Client::send is a blocking call.
            // To use it in an async context (like the `fetch` FFI which might be called from an async Wasm module),
            // it's better to use an async reqwest client and await the send.
            // However, Wasmtime's func_wrap currently expects a synchronous function.
            // For now, we'll keep the blocking client, but this is a point of attention if the Wasm host itself is async.
            // A common pattern is to use `tokio::task::block_in_place` or `spawn_blocking` if this needs to be truly non-blocking
            // with respect to the host's executor, but that adds complexity.
            // Given the current structure, the blocking client is consistent with its previous usage in main.rs.

            // We need to handle async reqwest client in a synchronous FFI function
            // Using tokio::task::block_in_place to execute async code in a blocking context

            // Use tokio's block_in_place to run the async operations in a blocking context
            let response = match tokio::task::block_in_place(move || {
                tokio::runtime::Handle::current().block_on(async { request_builder.send().await })
            }) {
                Ok(response) => response,
                Err(e) => {
                    let error_response = WasmFetchResponse {
                        status: 0, // 0 indicates network error or failed request
                        headers: HashMap::new(),
                        body: String::new(),
                        error: Some(WasmFetchError {
                            code: "FETCH_FAILED".to_string(),
                            message: format!("HTTP request execution failed: {}", e),
                        }),
                    };
                    
                    let error_json = serde_json::to_vec(&error_response)
                        .map_err(|e| anyhow!("fetch: failed to serialize error response to JSON: {}", e))?;
                        
                    if error_json.len() > resp_buf_max_len as usize {
                        return Ok(-(error_json.len() as i32));
                    }
                    
                    let memory_data_mut = memory.data_mut(&mut caller);
                    let response_target_slice = memory_data_mut
                        .get_mut(resp_buf_ptr as usize..(resp_buf_ptr as usize + error_json.len()))
                        .ok_or_else(|| {
                            anyhow!("fetch: response buffer pointer/length out of bounds for writing error")
                        })?;
                    
                    response_target_slice.copy_from_slice(&error_json);
                    return Ok(error_json.len() as i32);
                }
            };

            let status_code = response.status().as_u16();
            let mut response_headers_map = HashMap::new();
            for (name, value) in response.headers().iter() {
                response_headers_map
                    .insert(name.to_string(), value.to_str().unwrap_or("").to_string());
            }

            // Get response body text using block_in_place again for the async text() method
            let response_body_text = tokio::task::block_in_place(move || {
                tokio::runtime::Handle::current().block_on(async { response.text().await })
            })
            .map_err(|e| anyhow!("fetch: failed to read response body as text: {}", e))?;

            let wasm_response = WasmFetchResponse {
                status: status_code,
                headers: response_headers_map,
                body: response_body_text,
                error: None,
            };

            let response_json_bytes = serde_json::to_vec(&wasm_response)
                .map_err(|e| anyhow!("fetch: failed to serialize response to JSON: {}", e))?;

            if response_json_bytes.len() > resp_buf_max_len as usize {
                // Return negative length if buffer is too small
                return Ok(-(response_json_bytes.len() as i32));
            }

            let memory_data_mut = memory.data_mut(&mut caller);
            let response_target_slice = memory_data_mut
                .get_mut(resp_buf_ptr as usize..(resp_buf_ptr as usize + response_json_bytes.len()))
                .ok_or_else(|| {
                    anyhow!("fetch: response buffer pointer/length out of bounds for writing")
                })?;

            response_target_slice.copy_from_slice(&response_json_bytes);
            Ok(response_json_bytes.len() as i32)
        },
    )?;

    Ok(())
}
