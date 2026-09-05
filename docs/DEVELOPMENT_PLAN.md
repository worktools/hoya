# 开发计划与 Issue 管理

评估日期：2026-09-06。状态：开发计划，未实施功能不得视作已完成。

GitHub 同步状态：**已同步并回读验证**：[worktools/hoya issues](https://github.com/worktools/hoya/issues) 共 7 项，附优先级/类型标签、A0/A1/A2 里程碑及双向依赖。完整结构化记录见 [issue-drafts.json](issue-drafts.json)。H 开头编号属于 Hosta，Y 开头属于 Hoya；这些是本地规划编号，不是 GitHub issue 号码。

Hosta 维护用户体验、应用/版本/发布、触发、构建和运维控制面；Hoya 独立维护 QuickJS/Wasmtime 执行服务、协议、隔离、host capabilities 和发布产物。通过版本化 HTTP 协议集成，不复制引擎源码，不建立跨仓库相对路径依赖。

## 交付原则

前期全部通过命令行验证，CLI/API 是主要操作入口。Hosta CLI 面向 agents：非交互、稳定 JSON、退出码、stdin/文件输入、明确版本及可恢复查询。UI 先只读展示应用/版本/运行状态，复杂交互待使用反馈后评估，不作为前期验收依赖。AI 与定时管理为可顺延增强。

## 交付阶段

| 阶段 | 用户可验收结果 | 退出条件 |
| --- | --- | --- |
| A0 双运行时执行基线 | 相同 JSON 服务由独立 Hoya 执行，故障不阻塞 Hosta | 协议与 ABI 固定；值绑定、资源预算、默认禁网、鉴权完成；双运行时契约与故障用例通过，基础 CLI 可用 |
| A1 开发者闭环 | 无 AI、无浏览器，通过 CLI 完成源码→版本→试运行→发布→调用→定位错误 | JS/Rust WASM 闭环、稳定 key、版本明确、可恢复创建与运行详情；MoonBit 通过兼容验证后开启 |
| A2 开源预览交付 | 干净环境可安装、升级、备份并参与贡献 | 持久化/恢复、真实 CI、发布兼容矩阵、文档与许可证选择完成；AI/定时管理为 P2 可顺延项 |

阶段是质量门槛，不承诺未经估算的日期。单个实现 PR 尽量在 1–3 个工作日内可独立评审；较大 issue 按验收条目拆子项，保留父项及依赖。CLI 命令/schema 设计和 fixture 准备可在 A0 期间推进，功能完成仍依赖真实 Hoya 验证。

## 管理规则

- P0：阻塞可信执行基线；P1：核心用户流程或预览版交付；P2：可延期增强。每项只有一个优先级、一个阶段、一个类型。
- issue 放在实现归属仓库；跨仓库需求用链接表达依赖，不在两边复制同一工作。
- 初始均不分配个人、不设假定日期；领取时指派负责人，每仓库建议至多 2 项正在实现，优先清除依赖。
- 开始前复核代码和依赖；完成时由 PR 关联 issue，附复现步骤、测试/演示和兼容影响。验收未全满足不关闭。
- 新 issue 必须包含用户问题、代码依据、验收和不做范围；重复项确认后链接原项关闭。本轮发布前两仓库已有 issue 数均为 0。
- 每轮发布检查未完成项和依赖，记录顺延原因；不以文档声称完成替代测试。此处为人工维护约定，未创建定时自动化。

## 待办明细

### Y1 · P0 · A0 · 定义独立引擎 v1 执行协议与双运行时 ABI

GitHub：[hoya #5](https://github.com/worktools/hoya/issues/5)

src/main.rs 提供 /execute/js 和 /execute/wasm；响应是 success/output，缺少 runId、artifact hash 和协议版本；WASM 将 main 返回值解释为 JSON 字符串指针。

验收：

- [ ] 在 Hoya 仓库维护版本化 schema、错误码及请求/响应 fixtures；携带 runId、runtime、artifactSha256、input、limits、capabilities，并验证实际执行内容的 hash。
- [ ] 明确 JS main(input, ctx) 的脚本/模块形式、Promise 支持边界；明确 WASM memory、输入获取、结果指针/长度或 NUL 结束协议及 UTF-8/JSON 校验；固定一个 ABI v1，避免 Hosta 猜测返回值。
- [ ] 两种运行时以一致的 JSON result、结构化 logs、metrics、终态和错误语义返回；缺入口、不可序列化返回值不能伪装成功。
- [ ] 规定兼容策略、能力发现和旧端点弃用路径；Hosta 消费发布的 schema/fixtures，不复制引擎实现。

依赖：无，可开始。

### Y2 · P0 · A0 · 通过值绑定传入 JavaScript 输入，避免 JSON 被解释为代码

GitHub：[hoya #6](https://github.com/worktools/hoya/issues/6)

src/js_engine/mod.rs 将 input_str 插入 JSON.parse 的 JavaScript 模板字符串；反引号、反斜线和模板表达式会改变解析结果。

验收：

- [ ] 通过 QuickJS 值绑定或安全的独立 JSON 参数传入 input/datasource，不拼接到可执行源码。
- [ ] 回归反引号、${...}、换行、反斜线、Unicode、嵌套对象和 null；传入与返回逐值一致，数据中的表达式不被求值。
- [ ] 同步与 Promise 入口都覆盖；错误返回保留原始分类而不把宿主异常当作用户 JSON。

依赖：无，可开始。

### Y3 · P0 · A0 · 补齐执行总预算、宿主缓冲上限和过载拒绝

GitHub：[hoya #7](https://github.com/worktools/hoya/issues/7)

execution.rs 的 semaphore 限制运行数但 acquire_owned 等待者未设上限；FFI stdout/stderr 是无字节上限 String；已有 JS interrupt/heap 和 WASM fuel/epoch/StoreLimits 应保留。

验收：

- [ ] 显式限制等待队列、排队时长、源码/模块大小、日志/结果字节数和总执行期限；排队过载返回稳定错误及可重试信息。
- [ ] 预算覆盖编译、guest 执行、微任务和 host I/O；请求取消后有界回收工作，不能只取消 HTTP 等待。
- [ ] 日志在追加前限额，结果在跨宿主复制前限额；模块缓存同时限制总字节数而非只有条目数。
- [ ] 死循环、Promise 链、内存增长、日志洪泛、超大结果、取消和并发过载用例均在预算内结束，之后健康检查及正常执行成功。

依赖：[Y1](https://github.com/worktools/hoya/issues/5)。

### Y4 · P0 · A0 · 统一 JS/WASM 默认禁网与受控出站策略

GitHub：[hoya #8](https://github.com/worktools/hoya/issues/8)

JS fetch 仅做少量主机名黑名单且完整读取响应后检查大小；WASM fetch 未见同等目标校验；legacy /execute 直接 reqwest::get 下载代码。

验收：

- [ ] 执行请求默认没有网络能力，JS/WASM 共同使用显式授权策略；legacy 远程代码下载默认关闭。
- [ ] 校验协议、端口、DNS 解析后的地址及每次重定向，覆盖私网、loopback、link-local、IPv6 和 DNS 重绑定；连接必须使用已校验目标。
- [ ] 流式读取时限制响应字节数、重定向次数及总超时，不先无限读入内存；代理行为显式配置。
- [ ] 使用本地受控 fixture 测试允许/拒绝、重定向到内网、超大/慢响应和 JS/WASM 一致性，不访问真实内网服务。

依赖：[Y1](https://github.com/worktools/hoya/issues/5)。

### Y5 · P1 · A0 · 提供独立部署的 engine 模式和完整执行入口鉴权

GitHub：[hoya #9](https://github.com/worktools/hoya/issues/9)

main.rs 仍把 UI/AppStorage 与引擎放在同一服务；require_auth 只包 /execute、/execute/js、/execute/wasm，UI POST /execute/:id 在外；缺 token 时绑定 0.0.0.0。

验收：

- [ ] engine 模式不加载应用管理 UI/存储，只提供执行、能力发现与健康接口；legacy demo 必须显式开启。
- [ ] 所有执行入口遵循同一鉴权；明确本地开发与部署模式，部署缺 token 拒绝启动，默认监听地址可配置。
- [ ] 交付非 root、只读根文件系统、无宿主目录挂载的资源受限部署样例；取消/崩溃的隔离策略经过测试。
- [ ] 验证错误 token、遗漏 token、旧 UI 路径均不能绕过；/ready 真实反映初始化状态。

依赖：[Y1](https://github.com/worktools/hoya/issues/5)。

### Y6 · P1 · A1 · 发布可互操作的 JS、Rust WASM 与 MoonBit 示例

GitHub：[hoya #10](https://github.com/worktools/hoya/issues/10)

当前 Hosta starter 返回 42，Hoya WASM ABI 将其作为指针；examples 尚未提供两仓库共享的 JSON 输入输出验收集。

验收：

- [ ] 提供无 AI 的 JSON 转换示例，JS 与 Rust WASM 对相同 fixture 返回相同 JSON，包含日志和失败样例。
- [ ] MoonBit 提供固定工具链和实测编译/调用示例；若未通过则能力声明为实验性且 Hosta 不显示为就绪。
- [ ] 说明 imports、输入获取、内存分配/释放、返回值生命周期、错误处理和不支持的能力。
- [ ] 在 CI 编译示例并以 HTTP v1 协议执行，Hosta 使用同一 artifacts/fixtures 验证兼容性。

依赖：[Y1](https://github.com/worktools/hoya/issues/5)。

### Y7 · P1 · A2 · 建立独立引擎发布、兼容矩阵与可信维护文档

GitHub：[hoya #11](https://github.com/worktools/hoya/issues/11)

Cargo.toml publish=false；已有 CI、Docker 工作流与 4 个 WASM 单测；KNOWN_GAPS.md 声称全部修复但引用不存在的 Hosta src/index.ts 等。

验收：

- [ ] 版本化发布引擎二进制/镜像与协议文件，提供 checksums、变更日志和支持的平台；是否另发 crate 独立决策，不强行改成库。
- [ ] CI 增加 JS、HTTP 契约、鉴权、限额、网络策略与双仓库兼容测试；固定工具链可从干净环境构建。
- [ ] 明确支持的 Hosta/Hoya/协议版本矩阵、升级与弃用策略；文档结论附代码和验证依据。
- [ ] 补齐贡献指南、安全报告渠道；维护者选定许可证后加入 LICENSE 和依赖许可核对。

依赖：[Y2](https://github.com/worktools/hoya/issues/6)、[Y3](https://github.com/worktools/hoya/issues/7)、[Y4](https://github.com/worktools/hoya/issues/8)、[Y5](https://github.com/worktools/hoya/issues/9)、[Y6](https://github.com/worktools/hoya/issues/10)。
