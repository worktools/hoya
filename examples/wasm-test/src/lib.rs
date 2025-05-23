//! A test WebAssembly module for Hoya
//!
//! This file demonstrates using the APIs available in the Hoya WebAssembly runtime

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

// Define the imports from Hoya environment
extern "C" {
    fn app_log(level_ptr: *const u8, level_len: u32, msg_ptr: *const u8, msg_len: u32);
    fn get_unixtime() -> u64;
    fn fetch(options_ptr: u32, options_len: u32, resp_buf_ptr: u32, resp_buf_max_len: u32) -> i32;
}

// Main entry point
#[no_mangle]
pub extern "C" fn _start() {
    // Test app_log
    log_message("INFO", "Hello from WebAssembly!");

    // Test get_unixtime
    let timestamp = unsafe { get_unixtime() };

    // Convert timestamp to a string for logging
    // This is a bit tricky without std or alloc, so we'll do it manually
    let mut buffer = [0u8; 20]; // Buffer for the timestamp string
    let mut pos = 0;

    let mut n = timestamp;
    if n == 0 {
        buffer[pos] = b'0';
        pos += 1;
    } else {
        let mut digits = [0u8; 20];
        let mut digit_count = 0;

        while n > 0 {
            digits[digit_count] = (n % 10) as u8 + b'0';
            digit_count += 1;
            n /= 10;
        }

        // Reverse the digits
        for i in 0..digit_count {
            buffer[pos] = digits[digit_count - 1 - i];
            pos += 1;
        }
    }

    // Log the timestamp
    let timestamp_msg = b"Current Unix timestamp: ";
    let mut full_msg = [0u8; 50]; // Buffer for the full message

    // Copy timestamp_msg to full_msg
    for i in 0..timestamp_msg.len() {
        full_msg[i] = timestamp_msg[i];
    }

    // Copy the timestamp digits to full_msg
    for i in 0..pos {
        full_msg[timestamp_msg.len() + i] = buffer[i];
    }

    // Log the full message
    unsafe {
        app_log(
            b"INFO".as_ptr(),
            4,
            full_msg.as_ptr(),
            (timestamp_msg.len() + pos) as u32,
        );
    }
}

// Helper function to log a message
fn log_message(level: &str, message: &str) {
    unsafe {
        app_log(
            level.as_ptr(),
            level.len() as u32,
            message.as_ptr(),
            message.len() as u32,
        );
    }
}
