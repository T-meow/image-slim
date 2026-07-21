# image-slim Agent 版本开发计划（Windows）

## 1. 背景与目标

本文用于后续独立对话开发 image-slim 的 Agent 调用能力。计划基于 `main` 分支根提交
`6b7ef0e`、应用版本 `0.1.0` 和现有 Windows Tauri 桌面版制定。

目标是在不操作 GUI、不读取图片二进制到模型上下文、不输出逐文件成功日志的前提下，让本地
Agent 通过稳定的结构化接口扫描和压缩图片。默认响应应保持在几 KB 内，即使队列达到
10,000 项，也只返回汇总、问题计数和可分页的失败详情。

成功标准：

- 普通 GUI 便携版继续保持单 EXE、现有界面和 IPC 行为不变。
- 新增独立的 `image-slim-agent.exe`，同时支持 JSON CLI 和 MCP stdio。
- Windows 安装包仍只有一个 `setup.exe`，安装后包含 GUI 与 Agent 两个程序。
- Agent 不启动 GUI、不运行后台服务、不监听网络端口，也不访问远端 API。
- GUI 与 Agent 复用同一套扫描、限制、调度、编解码和安全输出实现。

## 2. 范围与非目标

本轮仅支持 Windows 10/11 x64，最低按 8 GiB 内存设计。继续使用 Rust 1.96、Node.js 24、
Tauri 2 和现有 PNG/JPEG/WebP 编解码器。

本轮不包含：

- Linux、macOS、ARM64、移动端或跨平台安装包。
- HTTP/SSE/Streamable HTTP 服务、局域网调用、账号、云端任务或遥测。
- Agent 控制 GUI、截图、窗口自动化、预览图返回或 base64 图片返回。
- 新图片格式、格式转换、自定义输出根目录、历史记录或队列持久化。
- 自动修改 Codex、Claude、VS Code 等宿主配置；只提供可复制的配置示例。

## 3. 发布形式

| 产物 | 用途 | 要求 |
| --- | --- | --- |
| `release/image-slim_<version>_x64-portable.exe` | 普通 GUI 便携版 | 单独运行，不依赖 Agent EXE |
| `release/image-slim-agent_<version>_x64.exe` | Agent/CLI 版 | 控制台子系统，不链接 Tauri/WebView |
| `release/image-slim_<version>_x64-setup.exe` | Windows 安装版 | 一个安装包，内部安装 GUI 与 Agent |
| `release/SHA256SUMS.txt` | 完整性校验 | 覆盖上述 EXE 与许可文件 |

安装后 `image-slim-agent.exe` 与 GUI 主程序位于同一安装目录，但不创建开始菜单入口、不注册
系统服务、不自动启动。MCP 宿主直接以完整路径启动 Agent EXE。

版本统一读取根目录 `package.json`。GUI EXE 继续要求不超过 15 MiB；Agent EXE 要求不超过
15 MiB；包含两者的 NSIS 要求不超过 10 MiB。超过时先记录依赖体积来源，再决定包级优化，
不得通过降低输出校验或压缩质量换体积。

## 4. 代码架构

将当前 Tauri 命令与压缩实现解耦，形成三个边界：

```text
image-slim GUI (Tauri adapter) ─┐
                               ├─ image-slim-core
image-slim-agent (CLI/MCP) ─────┘
```

推荐在 `src-tauri/Cargo.toml` 中建立 Cargo workspace，并新增：

- `src-tauri/crates/image-slim-core`：模型、错误、限制、扫描、调度、编解码、元数据和安全输出。
- `src-tauri/crates/image-slim-agent`：JSON CLI、MCP stdio、计划与任务生命周期。
- 现有 `src-tauri` 包继续负责 Tauri 状态、事件和窗口相关命令。

核心 crate 必须满足：

- 不依赖 `tauri`、WebView、对话框插件或 MCP SDK。
- 公开 `scan`、`run_batch`、`cancel`、`capabilities` 等面向服务的 API。
- 使用 `EventSink`/回调接口发布 `ScanEvent`、`ItemProgress` 和 `BatchSummary`。
- 使用共享取消令牌，不把 Tauri `AppHandle` 传入核心逻辑。
- 现有 Rust/TypeScript serde 类型继续由同一来源生成，GUI IPC 字段不发生破坏性变化。

Tauri adapter 将核心事件转发为当前 `scan-event`、`batch-item`、`batch-summary`；Agent adapter
则只维护本地状态和汇总，不向模型推送高频逐项事件。

## 5. Agent 进程与状态

Agent 是按需启动的本地进程。MCP 模式由宿主通过 stdin/stdout 管理生命周期；CLI 模式执行一次
请求后退出。

MCP 进程内状态限制：

- 扫描计划最多保留 4 个，30 分钟未使用即过期，按 LRU 淘汰。
- 同时最多运行 1 个批处理；单批内部继续由 `WorkScheduler` 控制最多两个任务并行。
- 最多保留 32 个已完成任务汇总，保留 60 分钟。
- 最多保留 128 个 `request_id` 的幂等结果，保留 60 分钟。
- 计划只保存路径、指纹和尺寸等清单，不缓存原始图片字节。
- 进程退出后计划和任务状态全部消失，不新增数据迁移或持久数据库。

新增 `agent_protocol_version: 1`。协议发生破坏性变化时递增该版本，不复用应用版本代替协议版本。

## 6. 公共工具接口

MCP 工具使用官方 Rust SDK `rmcp` 的锁定版本和 stdio transport。`structuredContent` 是权威结果，
每个工具声明 `inputSchema` 与 `outputSchema`；文本内容只提供一行摘要，不复制大数组。参考：

- <https://modelcontextprotocol.io/specification/2025-06-18/server/tools>
- <https://modelcontextprotocol.io/specification/2025-06-18/basic/transports>
- <https://github.com/modelcontextprotocol/rust-sdk>

MCP 与 JSON CLI 的业务结果统一使用信封，成功为
`{"ok":true,"result":{...}}`，失败为
`{"ok":false,"error":{"code":...,"params":...,"path":...,"detail":...,"retryable":...}}`。
MCP 业务失败同时设置 `isError: true`，但不退化为仅文本错误；输入无法解析、未知工具等协议错误仍使用
JSON-RPC error。每个工具的 `outputSchema` 同时覆盖成功和失败信封。

只暴露以下五个粗粒度工具，避免工具描述本身浪费上下文：

### `image_slim_capabilities`

只读、幂等、无参数。返回协议版本、应用版本、支持格式、扩展名、压缩档位、元数据策略、输入限制、
已配置允许根目录和是否允许覆盖。

### `image_slim_plan`

只读扫描。输入：

- `request_id`：必填 UUID，用于幂等。
- `paths`：1 至 1,000 个 Windows 文件或目录绝对路径。
- `output_subfolder`：默认 `compressed`，仅用于提前验证输出映射。
- `issue_limit`：默认 10，最大 50。

返回：`plan_id`、过期时间、已访问数、接受数、输入总字节、格式计数、问题码计数、最多
`issue_limit` 个问题、`next_issue_cursor` 和是否达到 10,000 项上限。完整 `InputItem` 清单不返回。

### `image_slim_compress`

有写入副作用。输入必须在 `plan_id` 与 `paths` 中二选一；直接提供路径时在服务端完成扫描，不把
扫描清单往返给模型。其他字段：

- `request_id`：必填 UUID；重复请求返回同一个 `job_id`，不得重复写入。
- `preset`：`lossless | balanced | strong`，默认 `balanced`。
- `output_mode`：`subfolder | overwrite`，默认 `subfolder`。
- `output_subfolder`：默认 `compressed`。
- `metadata_policy`：`essential | supported`，默认 `essential`。
- `allow_conflicts`：默认 `false`。
- `wait_ms`：默认 1,000，最大 5,000；短任务可在一次调用内直接返回最终汇总。

返回：`job_id`、`state`、总数、已完成数、无需替换数、失败数、取消数、输入/输出/节省字节和
问题码计数。处理中不返回逐文件成功记录。

### `image_slim_status`

只读、幂等。输入 `job_id`、`issue_cursor`（默认 0）和 `issue_limit`（默认 10，最大 50）。返回固定
大小的当前计数、状态、下一问题游标和本页问题。完成后返回 `BatchSummary`。

### `image_slim_cancel`

幂等。输入 `job_id`，返回 `accepted` 与当前状态。已完成或已取消任务再次调用不得报内部错误。

## 7. JSON CLI

同一 Agent EXE 提供以下子命令，作为 MCP 之外的低开销回退接口和协议测试入口：

```powershell
image-slim-agent.exe capabilities --json
image-slim-agent.exe --allow-root D:\Pictures plan --request -
image-slim-agent.exe --allow-root D:\Pictures compress --request -
image-slim-agent.exe --allow-root D:\Pictures mcp
```

`--request -` 表示从 stdin 读取一个 UTF-8 JSON 对象；stdout 只输出一个 JSON 结果。MCP 模式
stdout 只允许协议帧。诊断、panic hook 和运行日志全部写 stderr，不得污染 stdout。

JSON CLI 是单次同步接口：`plan` 只返回扫描摘要，不返回可跨进程复用的 `plan_id`；`compress`
只接受 `paths` 并阻塞到最终汇总。`status`、`cancel` 只作为 MCP 工具存在，因为任务状态不落盘，
CLI 进程退出后不能跨进程查询或取消。`--allow-root` 可重复，`--allow-overwrite` 是全局开关。

## 8. Token 与响应预算

- `tools/list` 的五个工具定义序列化后合计不超过 16 KiB。
- capabilities、无问题的 plan 和 status 默认响应分别不超过 8 KiB。
- 任意默认工具响应设 32 KiB 硬上限；问题通过游标继续获取。
- 成功项目不返回路径；只有问题项返回路径和结构化 `AppError`。
- `detail` 默认省略，只有输入 `include_technical_detail: true` 时返回。
- 不发送逐文件 MCP progress notification；Agent 按需要调用 status。
- 不返回图片像素、缩略图、预览缓存路径、base64、EXIF 原文或完整调试日志。
- 所有字节数使用整数，汇总直接提供 `saved_bytes`，不要求模型自行计算。

## 9. 权限与输出安全

Agent 启动时通过一个或多个 `--allow-root <absolute-path>` 设置访问根目录。未配置根目录时可以调用
capabilities，但所有扫描和写入请求返回 `root_not_allowed`。

必须新增并本地化以下错误码：

- `invalid_request`
- `root_not_allowed`
- `overwrite_not_allowed`
- `plan_expired`
- `job_not_found`

其他安全规则：

- 所有输入先规范化和 canonicalize，再按 Windows 不区分大小写语义检查允许根目录。
- 继续拒绝符号链接、路径穿越、格式伪造、源文件变化和输出冲突。
- 默认只写入输入根目录下的 `compressed` 子目录。
- 覆盖模式必须同时满足进程启动参数 `--allow-overwrite` 和请求 `output_mode=overwrite`；缺一即拒绝。
- 不允许 Agent 通过请求扩大根目录、修改限制、关闭内容哈希或跳过原子写入 guard。
- 保留 512 MiB、100MP、65,535 边长、10,000 项和动态内存预算的二次校验。
- 失败或取消时沿用现有临时文件清理与目标文件保护语义。

## 10. 实施阶段

1. **核心解耦**：建立 core crate 和事件接收接口；Tauri adapter 接回现有 IPC。全部现有测试通过后再继续。
2. **JSON CLI**：实现 Agent 状态、允许根目录、计划、幂等、批处理、状态和取消；先用 CLI 集成测试验证。
3. **MCP stdio**：引入并精确锁定官方 `rmcp`，映射五个工具及 Schema；增加 stdout 纯净度测试。
4. **安装与归档**：构建 Agent 控制台 EXE，将其加入 NSIS，并扩展 `stage-release.mjs` 与哈希清单。
5. **文档与 CI**：增加 Agent 配置示例、协议说明和 Windows CI 的 Agent 构建/协议测试。

每阶段单独提交。核心解耦提交不得同时改变 GUI 文案、布局、压缩参数或输出策略，以便出现回归时能
精确定位。

## 11. 测试与验收

- 原有前端、Rust、IPC bindings、CSP、版本和 Tauri release 检查全部继续通过。
- core 测试不依赖 Tauri runtime；GUI adapter 测试验证三类事件与原协议一致。
- CLI/MCP 覆盖空请求、无权限根目录、重复 `request_id`、计划过期、任务不存在和取消幂等。
- 10,000 项扫描不返回清单，默认响应满足大小预算，Agent 进程内存不保留图片字节。
- 覆盖模式双重授权、子目录输出、冲突、源文件变化、同大小同时间戳内容变化和临时文件清理通过。
- 快速重复 status 不产生完整历史；问题游标无重复、无遗漏并有上限。
- MCP 测试进程验证 stdout 每一行都是合法协议消息，stderr 日志不会进入协议流。
- GUI EXE、Agent EXE、NSIS 的体积和 SHA-256 写入 `release/SHA256SUMS.txt` 并复核。
- 从全新目录分别启动 GUI 便携版和 Agent CLI；Agent 不创建窗口，GUI 不依赖 Agent EXE。
- 安装版安装后两个程序存在，卸载后均被移除，不留下服务、启动项或监听端口。

## 12. 兼容性与迁移

- 现有 Tauri 命令名、TypeScript IPC 类型、localStorage 键、元数据策略和输出目录语义保持兼容。
- Agent 与 GUI 使用相同应用版本；Agent 协议另有 `agent_protocol_version`。
- 计划、任务和幂等缓存只存在于进程内，无磁盘 Schema 或升级迁移。
- 不改变当前远端发布流程，不自动创建 GitHub Release，也不提交 `release/` 二进制。

## 13. 新对话启动检查清单

下一次对话开始开发时：

1. 读取工作区 `AGENTS.md`、本文件和 `docs/optimization-plan.zh-CN.md`。
2. 执行 `git status -sb`，确认没有其他未提交修改；本文件若尚未提交，先按用户要求处理。
3. 核对 `main` 的最新提交和 CI 状态，不假设仍停留在 `6b7ef0e`。
4. 先写核心拆分的具体文件迁移计划，再实施阶段 1；不要同时开始 MCP 和打包。
5. 本轮严格限定 Windows x64，不顺带实现 Linux/macOS 抽象或产物。

## 14. 实施结果（2026-07-21）

- 已建立 Cargo workspace；`image-slim-core` 不依赖 Tauri，GUI 通过 `EventSink` adapter 保持原命令、
  事件名和 IPC 字段。Agent 计划额外保存 BLAKE3 源哈希，不保留图片字节。
- 已实现同步 JSON CLI、允许根目录/覆盖双重授权、计划/任务/幂等 LRU 与 TTL、五个 MCP stdio
  工具、统一结果信封、问题分页和响应预算。`rmcp` 精确锁定为 `2.2.0`，未启用 HTTP transport。
- 已增加 `docs/agent-protocol.zh-CN.md`，包含 CLI、MCP、Codex 与 Claude Desktop 手工配置示例；
  未修改任何宿主配置。
- 前端 8 个测试文件共 19 项通过；Cargo workspace 共 48 项测试通过，1 项 release 性能测试按设计
  忽略。`fmt`、`clippy -D warnings`、workspace test/check、IPC、版本、配置和前端生产构建通过。
- `tools/list` 五工具合计小于 16 KiB；10,000 项计划汇总、分页预算、同大小同时间戳源变化、
  幂等、计划过期、无权限根目录和 MCP stdout 纯净度均有自动测试。
- GUI 便携版：`release/image-slim_0.1.0_x64-portable.exe`，9,920,000 字节，SHA-256
  `9bb5fe11447db11f8fce849f8d915a9b970353105b13f65d0d44b453913be1d5`。
- Agent：`release/image-slim-agent_0.1.0_x64.exe`，6,267,392 字节，SHA-256
  `692539a6f014787332b5514623bc2cde5f77466e1a3f93259dffc6dd5e2e0dbb`。
- NSIS：`release/image-slim_0.1.0_x64-setup.exe`，4,560,113 字节，SHA-256
  `b8e65caccae89d3d6869cd85c54164ae20e4dd2d92b922714c2fa23bfe8591d4`。
- 最终安装包已静默安装到隔离目录，确认 GUI、Agent 与卸载器同目录且 Agent capabilities 正常；
  静默卸载后目录移除。GUI 便携版也已在不含 Agent 的独立目录启动通过。
