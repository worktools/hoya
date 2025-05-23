//! Test WebAssembly file for stdout and stderr capturing
//!
//! This file demonstrates how to use stdout and stderr in WebAssembly
//! modules compiled from Rust.

use std::fmt;

// Forward declaration of the functions imported from the host
extern "C" {
    fn capture_stdout(ptr: *const u8, len: usize);
    fn capture_stderr(ptr: *const u8, len: usize);
    fn app_log(level_ptr: *const u8, level_len: usize, msg_ptr: *const u8, msg_len: usize);
}

// Simple struct to demonstrate the standard formatting of complex types
struct Point {
    x: i32,
    y: i32,
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Point({}, {})", self.x, self.y)
    }
}

// Print to stdout (captured)
fn print_stdout(msg: &str) {
    unsafe {
        capture_stdout(msg.as_ptr(), msg.len());
    }
}

// Print to stderr (captured)
fn print_stderr(msg: &str) {
    unsafe {
        capture_stderr(msg.as_ptr(), msg.len());
    }
}

// Log function using app_log
fn log(level: &str, msg: &str) {
    unsafe {
        app_log(
            level.as_ptr(),
            level.len(),
            msg.as_ptr(),
            msg.len(),
        );
    }
}

// Entrypoint for WebAssembly module
#[no_mangle]
pub extern "C" fn _start() {
    // Output to stdout
    print_stdout("This is a standard output message from WASM");
    print_stdout("This is another standard output message from WASM");
    
    // Output to stderr
    print_stderr("This is an error message from WASM");
    print_stderr("This is another error message from WASM");
    
    // Output complex types
    let point = Point { x: 10, y: 20 };
    print_stdout(&format!("Complex type output: {}", point));
    
    // Use app_log
    log("INFO", "This is a log message via app_log from WASM");
    log("ERROR", "This is an error message via app_log from WASM");
}

// Required for WebAssembly modules to export memory
#[no_mangle]
static mut MEMORY: [u8; 65536] = [0; 65536];
