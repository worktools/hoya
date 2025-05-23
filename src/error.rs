//! # Error handling for the Hoya service
//!
//! This module provides error types and response structures for the Hoya service.
//! It includes error handling for JavaScript execution, WebAssembly execution,
//! HTTP requests, and general application errors.

use anyhow::Error as AnyhowError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// Define these types directly in this module
/// Error information returned to API clients
#[derive(serde::Serialize, Debug)]
pub struct ErrorInfo {
    /// Error code identifier
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Optional additional details about the error
    pub details: Option<HashMap<String, serde_json::Value>>,
}

/// Metadata about code execution
#[derive(serde::Serialize, Debug)]
pub struct ExecutionMetadata {
    /// Execution time in milliseconds
    pub execution_time: u64,
    /// Type of code executed ("javascript" or "webassembly")
    pub code_type: String,
    /// ISO timestamp of execution
    pub timestamp: String,
    /// Size of the executed code in bytes
    pub resource_size: usize,
}

/// Response for the execute endpoint
#[derive(serde::Serialize, Debug)]
pub struct ExecuteResponse {
    /// Status of execution ("success" or "error")
    pub status: String,
    /// Output from code execution (if successful)
    pub output: Option<String>,
    /// Error information (if execution failed)
    pub error: Option<ErrorInfo>,
    /// Metadata about the execution
    pub metadata: ExecutionMetadata,
}

/// Application error types
///
/// This enum represents the different kinds of errors that can occur
/// during code execution in the Hoya service.
#[derive(Debug)]
pub enum AppError {
    /// QuickJS JavaScript engine errors
    QuickJs(rquickjs::Error),
    /// Wasmtime WebAssembly engine errors
    Wasmtime(AnyhowError),
    /// HTTP request errors
    Reqwest(reqwest::Error),
    /// Internal application errors
    Internal(String),
}

impl From<rquickjs::Error> for AppError {
    fn from(err: rquickjs::Error) -> Self {
        AppError::QuickJs(err)
    }
}

impl From<AnyhowError> for AppError {
    fn from(err: AnyhowError) -> Self {
        AppError::Wasmtime(err)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Reqwest(err)
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Internal(s.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status_code, error_info) = match self {
            AppError::QuickJs(e) => {
                let mut details = HashMap::new();
                details.insert(
                    "errorType".to_string(),
                    serde_json::Value::String("QuickJS".to_string()),
                );

                let error = ErrorInfo {
                    code: "JAVASCRIPT_EXECUTION_ERROR".to_string(),
                    message: format!("JavaScript Execution Error: {}", e),
                    details: Some(details),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, error)
            }
            AppError::Wasmtime(e) => {
                let mut details = HashMap::new();
                details.insert(
                    "errorType".to_string(),
                    serde_json::Value::String("Wasmtime".to_string()),
                );

                let error = ErrorInfo {
                    code: "WEBASSEMBLY_EXECUTION_ERROR".to_string(),
                    message: format!("WebAssembly Execution Error: {}", e),
                    details: Some(details),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, error)
            }
            AppError::Reqwest(e) => {
                let mut details = HashMap::new();
                if let Some(url) = e.url().map(|u| u.to_string()) {
                    details.insert("url".to_string(), serde_json::Value::String(url));
                }
                if let Some(status) = e.status() {
                    details.insert(
                        "status".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(status.as_u16())),
                    );
                }

                let error = ErrorInfo {
                    code: "FETCH_ERROR".to_string(),
                    message: format!("Failed to fetch resource: {}", e),
                    details: Some(details),
                };
                (StatusCode::BAD_GATEWAY, error)
            }
            AppError::Internal(s) => {
                let error = ErrorInfo {
                    code: "INTERNAL_ERROR".to_string(),
                    message: s,
                    details: None,
                };
                (StatusCode::INTERNAL_SERVER_ERROR, error)
            }
        };

        // Generate current timestamp in ISO format
        let now = SystemTime::now();
        let timestamp = match now.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(
                    duration.as_secs() as i64,
                    duration.subsec_nanos(),
                )
                .unwrap_or_else(|| chrono::Utc::now());
                datetime.to_rfc3339()
            }
            Err(_) => chrono::Utc::now().to_rfc3339(),
        };

        let metadata = ExecutionMetadata {
            execution_time: 0, // We don't have execution time for errors before execution
            code_type: "unknown".to_string(),
            timestamp,
            resource_size: 0, // No resource size for errors before loading
        };

        let body = Json(ExecuteResponse {
            status: "error".to_string(),
            output: None,
            error: Some(error_info),
            metadata,
        });

        (status_code, body).into_response()
    }
}
