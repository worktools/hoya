# Hoya / Hosta 集成 — 已知功能缺失清单

本文档记录代码审查中发现、但未在本轮修复的问题。"严重"分组的 4 项已修复（见下方"已修复"），其余按优先级列出，供后续迭代参考。

## ✅ 已修复（严重）

1. **JS 无 wall-clock 超时** — `js_engine/mod.rs` 通过 `Runtime::set_interrupt_handler` 加了 5 秒执行预算，超时抛出可捕获的中断异常。
2. **JS 不支持真正的 async/await** — 改为在 JS 侧用 `.then()` 捕获 Promise 结算状态，Rust 侧通过 `Runtime::execute_pending_job()` 排空微任务队列后读回结果；仅支持不依赖真实异步 I/O（如 fetch/定时器）的、可同步落定的 Promise。
3. **hosta 进程退出不清理 hoya 子进程** — `src/index.ts` 新增 `SIGTERM`/`SIGINT` 处理器，调用 `stopHoya()`。
4. **无鉴权 + 无请求体大小限制** — hoya 新增 `HOYA_AUTH_TOKEN` 共享密钥鉴权（`/execute*` 端点）+ 显式 16MB 请求体上限；`hoya-client.ts` 自动生成随机 token 并注入子进程环境变量 + 请求头。
5. **（连带发现）hoya 端口硬编码为 3000，完全无视 `PORT` 环境变量** — 导致 Hosta 传的 `PORT=4300` 被忽略，集成完全无法工作。已改为读取 `PORT` 环境变量。

## 🟠 高优先级（未修复）

| 问题 | 位置 | 说明 |
|------|------|------|
| 并发启动竞态 | `hosta/src/hoya-client.ts` `ensureHoyaClient()` | `if (!hoyaClient)` 无锁保护，并发请求可能同时 spawn 两个 hoya 实例抢占端口 |
| hoya 崩溃无自动重启 | `hosta/src/hoya-client.ts` | `hoyaProcess.on('exit')` 只记日志不重启，之后所有请求持续失败直到手动重启 Hosta |
| WASM 内存页数限制未真正生效 | `hoya/src/wasm_engine/mod.rs` | wasmtime 33 没有 `wasm_memory_limits` API，模块可自行 export 更大内存，目前仅靠 fuel 间接限流 |
| JS 无内存限制之外的 GC 压力控制 | `hoya/src/js_engine/mod.rs` | 已加 `set_memory_limit(64MB)`，但未设置 `set_gc_threshold`，大对象分配可能造成 GC 抖动 |
| JS `fetch` 是假的 | `hoya/src/js_engine/ffis.rs` | 直接 `throw FETCH_NOT_IMPLEMENTED`，与 WASM 侧完整实现的 fetch 不对等，用户代码里用了 fetch 会直接报错（而不是网络请求） |
| WASM fetch 可能被 fuel 中断出半截请求 | `hoya/src/wasm_engine/ffis.rs` | `tokio::task::block_in_place` 包装的异步 fetch 如果 fuel 在请求过程中耗尽，可能留下未完成的网络请求 |

## 🟡 中优先级（界面/流程）

| 问题 | 位置 | 说明 |
|------|------|------|
| hoya 执行结果页面缺少 stdout/stderr 展示 | `hoya/templates/execute.html` | 只 `JSON.stringify` 最终结果，用户看不到 `console.log`/`ctx.log()` 输出 |
| Hosta 前端不显示当前沙箱级别 | `hosta/frontend/src` | 切换 `HOYA_ENABLED` 后用户界面看不出代码跑在 vm.createContext（①级）还是 hoya rquickjs（②级） |
| 无法在界面里切换引擎 | — | `HOYA_ENABLED` 只能改环境变量重启服务，不符合"低门槛"产品定位 |
| metadata 未在前端渲染 | `hosta/src/executor.ts` + 前端 | `execution_time`/`resource_size` 等已经算出来但没有展示 |
| hoya 无 API 文档端点 | `hoya/src/main.rs` | 没有 `/docs` 或自描述端点，接入方只能看源码 |
| 遗留的 `server.mjs` 与 `src/` 重复 | hosta 根目录 | `server.mjs` 是旧架构的单文件版本，`package.json` 的 `start` 脚本实际运行 `dist/index.js`（编译自 `src/`）。`server.mjs` 目前只在 RFC 文档中被引用，属于死代码，容易誤导后续开发者以为它是入口 |

## 备注

- Promise 支持目前只覆盖"同步可落定"的场景（`async function main() { return x; }`、连续 `await` 已解决的 Promise 等）。真正依赖网络/定时器的异步操作仍不支持，会在耗尽微任务队列后返回 `Promise did not resolve` 错误。
- `HOYA_AUTH_TOKEN` 未设置时 hoya 会打印 WARN 日志但仍允许匿名访问（本地开发模式），生产部署必须设置该环境变量。
