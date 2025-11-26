# Hoya 错误页面 JSON 响应设计

## 🎯 目标
所有 HTTP 错误都返回结构化的 JSON 响应，而不是默认的 HTML 错误页面。

## 📋 错误响应格式

### 标准错误格式
```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human readable error message",
    "details": {
      "type": "error_type",
      "description": "Detailed error description"
    }
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

## 🚨 错误类型

### 1. 404 Not Found
**触发条件**: 访问不存在的端点
**错误代码**: `NOT_FOUND`
**HTTP状态码**: 404

```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "The requested resource was not found",
    "details": {
      "type": "route_not_found",
      "description": "This endpoint does not exist. Available endpoints: /execute, /health, /ready"
    }
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

### 2. 执行端点错误

#### 空 URL 错误
**错误代码**: `INTERNAL_ERROR`
**HTTP状态码**: 500

```json
{
  "error": {
    "code": "INTERNAL_ERROR",
    "message": "URL cannot be empty",
    "details": null
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

#### 不支持的文件类型
**错误代码**: `INTERNAL_ERROR`
**HTTP状态码**: 500

```json
{
  "error": {
    "code": "INTERNAL_ERROR",
    "message": "Unsupported file extension. Only .js and .wasm are supported.",
    "details": null
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

#### 下载失败
**错误代码**: `FETCH_ERROR`
**HTTP状态码**: 502

```json
{
  "error": {
    "code": "FETCH_ERROR",
    "message": "Failed to fetch resource: HTTP status 404",
    "details": {
      "url": "https://example.com/nonexistent.js",
      "status": 404
    }
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

### 3. JavaScript 执行错误
**错误代码**: `JAVASCRIPT_EXECUTION_ERROR`
**HTTP状态码**: 500

```json
{
  "error": {
    "code": "JAVASCRIPT_EXECUTION_ERROR",
    "message": "JavaScript Execution Error: ReferenceError: undefinedVariable is not defined",
    "details": {
      "errorType": "QuickJS"
    }
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

### 4. WebAssembly 执行错误
**错误代码**: `WEBASSEMBLY_EXECUTION_ERROR`
**HTTP状态码**: 500

```json
{
  "error": {
    "code": "WEBASSEMBLY_EXECUTION_ERROR",
    "message": "WebAssembly Execution Error: Invalid WASM module",
    "details": {
      "errorType": "Wasmtime"
    }
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

### 5. 中间件错误
**错误代码**: `MIDDLEWARE_ERROR`
**HTTP状态码**: 500

```json
{
  "error": {
    "code": "MIDDLEWARE_ERROR",
    "message": "Service error occurred",
    "details": {
      "type": "middleware_error",
      "description": "Tower middleware error details"
    }
  },
  "status": "error",
  "timestamp": "2024-01-20T10:30:00Z"
}
```

## 🧪 测试方法

### 使用 curl 测试
```bash
# 测试 404 错误
curl -s http://localhost:3000/nonexistent | jq .

# 测试空 URL
curl -s -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": ""}' | jq .

# 测试不支持的文件类型
curl -s -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/test.txt"}' | jq .

# 测试无法访问的 URL
curl -s -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "https://nonexistent-domain-12345.com/test.js"}' | jq .
```

### 使用测试脚本
```bash
./test_error_pages.sh
```

## 🔧 实现细节

### 错误处理组件

1. **AppError 枚举** (`src/error.rs`)
   - 定义了所有可能的错误类型
   - 实现了 `IntoResponse` trait 用于 JSON 响应

2. **全局错误处理** (`src/main.rs`)
   - `not_found_handler()` - 处理 404 错误
   - `middleware_error_handler()` - 处理中间件错误
   - `handle_error()` - 处理通用错误

3. **错误响应结构**
   - 包含错误代码、消息、详情
   - 统一的时间戳格式
   - 一致的 JSON 结构

## 📊 优势

1. **一致性** - 所有错误都使用相同的 JSON 格式
2. **可调试** - 详细的错误信息和上下文
3. **机器可读** - 便于前端处理和显示
4. **用户友好** - 清晰的错误消息
5. **标准化** - 遵循 RESTful API 错误处理最佳实践