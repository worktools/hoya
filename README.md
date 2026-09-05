# Hoya

> 新版默认启动独立 engine 模式，要求 `HOYA_AUTH_TOKEN`，提供 `/v1/executions` 和 `/v1/capabilities`。协议、资源边界、JS/Rust 示例及验证命令见 [Engine v1](docs/engine-v1.md)。以下原有 UI/URL 执行说明仅适用于显式设置 `HOYA_MODE=legacy-demo` 的开发模式。

> 2026-09-06 维护方向：Hoya 作为独立 QuickJS/Wasmtime 执行引擎，为 Hosta 等客户端提供版本化协议。Hosta 的 CLI 集成正在配套 PR 中推进；后续阶段、验收与 7 项 GitHub issues见 [开发计划](docs/DEVELOPMENT_PLAN.md)。`KNOWN_GAPS.md` 的旧集成结论已标记为历史记录。

A serverless execution engine for dynamically running JavaScript and WebAssembly scripts fetched from remote URLs. Hoya provides a sandboxed environment, captures standard I/O, and injects limited host functionalities.

Refer to [REQUIREMENTS.md](REQUIREMENTS.md) for detailed project requirements and functional specifications.

## Getting Started

### Prerequisites

- Rust toolchain

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/hoya.git
cd hoya

# Build the project
cargo build
```

### Running the Server

```bash
cargo run
```

This will start the server on `http://127.0.0.1:3000`.

## Usage

Hoya exposes a single endpoint `/execute` which takes a JSON payload with a `url` field pointing to a JavaScript or WebAssembly file.

### Example

```bash
curl -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/your-script.js"}'
```

## Testing

Refer to the test scripts (`test_hoya.sh`, `test_stdout_stderr.sh`) and the `examples/` directory for testing various features.

## License

MIT License (see LICENSE file for details).
