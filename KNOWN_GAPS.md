# Hoya / Hosta 集成 — 已知功能缺失清单

本文档记录代码审查中发现的问题。所有问题已全部修复。

## ✅ 已修复（严重）

1. **JS 无 wall-clock 超时** — `js_engine/mod.rs` 通过 `Runtime::set_interrupt_handler` 加了 5 秒执行预算，超时抛出可捕获的中断异常。
2. **JS 不支持真正的 async/await** — 改为在 JS 侧用 `.then()` 捕获 Promise 结算状态，Rust 侧通过 `Runtime::execute_pending_job()` 排空微任务队列后读回结果；仅支持不依赖真实异步 I/O（如 fetch/定时器）的、可同步落定的 Promise。
3. **hosta 进程退出不清理 hoya 子进程** — `src/index.ts` 新增 `SIGTERM`/`SIGINT` 处理器，调用 `stopHoya()`。
4. **无鉴权 + 无请求体大小限制** — hoya 新增 `HOYA_AUTH_TOKEN` 共享密钥鉴权（`/execute*` 端点）+ 显式 16MB 请求体上限；`hoya-client.ts` 自动生成随机 token 并注入子进程环境变量 + 请求头。
5. **（连带发现）hoya 端口硬编码为 3000，完全无视 `PORT` 环境变量** — 导致 Hosta 传的 `PORT=4300` 被忽略，集成完全无法工作。已改为读取 `PORT` 环境变量。

## ✅ 已修复（高优先级）

6. **并发启动竞态** — `hosta/src/executor.ts` 的 `ensureHoyaClient()` 改为共享一个启动 Promise，并发调用者都等待同一次 `startHoya()`，失败时清空 Promise 允许重试。
7. **hoya 崩溃无自动重启** — `hosta/src/hoya-client.ts` 新增 `intentionalStop` 标记区分主动停止与意外崩溃；崩溃后按指数退避（1s→2s→4s...最长 30s）自动重启，连续失败 5 次后放弃并打日志。
8. **WASM 内存页数限制未真正生效** — 通过 wasmtime 的 `StoreLimits`/`Store::limiter` 强制 32MB 单实例内存上限（`memory.grow` 超限时触发 trap），而非依赖模块自身声明的内存类型。
9. **JS 无 GC 压力控制** — 新增 `runtime.set_gc_threshold()`，在达到内存上限 1/4 时更积极地触发 GC。
10. **JS `fetch` 是假的** — 改为真实实现：复用 WASM 侧的 SSRF 黑名单（拦截 `127.0.0.1`/`localhost`/`::1`/`0.0.0.0`）、5 秒超时、512KB 响应体上限，返回与 WASM 一致的 `{ok,data}`/`{ok:false,error}` JSON 信封。
11. **WASM `fetch` 无超时** — 审查中发现请求构造完全没有设置超时（此前文档误判为"被 fuel 打断"，实际风险更大：慢速/无响应服务器会无限期占用 worker 线程）。已加 5 秒超时，与 JS 侧一致。

## ✅ 已修复（中优先级 — 界面/流程）

12. **遗留的 `server.mjs` 死代码** — 已删除。旧架构的单文件版本，`package.json` 的 `start` 脚本实际运行 `dist/index.js`（编译自 `src/`）。
13. **hoya 无 API 文档端点** — 新增 `GET /api` 自描述端点，返回所有路由、方法、参数说明、资源限制和鉴权信息。
14. **hoya 执行结果页面缺少 stdout/stderr 展示 + metadata** — `execute.html` 模板已重构：展示执行时间、引擎类型、stdout 日志（暗色终端风格）、返回值、完整 JSON 响应（可折叠）。
15. **Hosta 前端不显示沙箱级别 + 无法切换引擎** — 侧边栏底部新增沙箱指示器（① vm / ② hoya），点击即可运行时切换，无需重启服务。后端新增 `POST /api/admin/sandbox` 切换端点。

## 备注

- Promise 支持目前只覆盖"同步可落定"的场景（`async function main() { return x; }`、连续 `await` 已解决的 Promise 等）。真正依赖网络/定时器的异步操作仍不支持，会在耗尽微任务队列后返回 `Promise did not resolve` 错误。
- `HOYA_AUTH_TOKEN` 未设置时 hoya 会打印 WARN 日志但仍允许匿名访问（本地开发模式），生产部署必须设置该环境变量。
- JS/WASM 的 `fetch` 现在都是「同步阻塞底层线程直到请求完成或超时」的实现（通过 `tokio::task::block_in_place` 逃逸 async 上下文），不是真正的非阻塞 I/O；高并发场景下可能耗尽 tokio 的阻塞线程池，需要在生产前评估请求量级。
