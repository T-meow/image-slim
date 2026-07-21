<p align="center">
  <img src="assets/icon.svg" width="88" alt="image-slim 图标">
</p>

<h1 align="center">image-slim</h1>

<p align="center">完全离线的 Windows 批量图片压缩器。</p>

<p align="center">
  <strong>简体中文</strong> · <a href="README.en.md">English</a>
</p>

<p align="center">
  <a href="https://t-meow.github.io/image-slim/"><strong>官网与下载</strong></a> ·
  <a href="https://github.com/T-meow/image-slim/releases/tag/v0.1.0">Release v0.1.0</a> ·
  <a href="https://github.com/T-meow/image-slim/issues">问题反馈</a>
</p>

image-slim 使用 Tauri 2、Svelte 5 与 Rust 构建，在本机压缩 PNG、JPEG 和 WebP，
不上传图片，不需要账号，也不包含遥测。当前版本为 `0.1.0`，仅支持 Windows 10/11 x64。

<p align="center">
  <img src="docs/Screenshot%202.png" width="100%" alt="image-slim 批量队列与压缩前后对比界面">
</p>

## 下载

| 版本 | 适用场景 | 下载 |
|---|---|---|
| Windows 安装版 | 推荐；安装 GUI 与 Agent | [`image-slim_0.1.0_x64-setup.exe`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/image-slim_0.1.0_x64-setup.exe) |
| GUI 便携版 | 免安装的单文件桌面程序 | [`image-slim_0.1.0_x64-portable.exe`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/image-slim_0.1.0_x64-portable.exe) |
| Agent 独立版 | JSON CLI / MCP stdio 自动化 | [`image-slim-agent_0.1.0_x64.exe`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/image-slim-agent_0.1.0_x64.exe) |

完整校验值见 [`SHA256SUMS.txt`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/SHA256SUMS.txt)。
`0.1.0` 尚未代码签名，Windows SmartScreen 可能显示未知发布者；可在下载后使用
`Get-FileHash <文件路径> -Algorithm SHA256` 核对文件。

## 特性与优势

| 特性 | 能力与优点 |
|---|---|
| 大文件与大批量 | **单文件最大 `512 MiB`**；单图最大 `100,000,000` 像素、单边最大 `65,535` 像素；单次队列最多 `10,000` 张图片。超限文件只会单独报错，不影响队列中的其他任务。 |
| 完全离线 | 图片、路径、预览和压缩结果始终留在本机；无需账号，不监听网络端口，也没有遥测、云同步或自动更新。 |
| 三种格式、三个档位 | 原生支持静态 PNG、JPEG、WebP，提供无损、均衡、强力档位；分别使用 OxiPNG、libimagequant、MozJPEG 与 libwebp 完成针对性优化。 |
| 不做负优化 | 每个候选结果都会重新解码并验证格式、尺寸和完整性；无损档还会核对解码像素。只有结果更小时才采用，否则保留或复制原图。 |
| 顺手的批处理 | 文件、文件夹可以混合拖入，目录会递归扫描并保留相对结构；支持虚拟化大队列、逐项失败重试、取消和整批节省空间统计。 |
| 内存感知并行 | 最多两个工作任务并行处理，并按图片格式和尺寸估算峰值内存，为 Windows 与 WebView 保留空间，减少大图批处理挤满内存的风险。 |
| 所见即所得 | 在写入前即可用滑杆对比原图与压缩结果，并在 `50%` 至 `200%` 范围缩放检查细节。 |
| 输出更安全 | 可写入自定义子文件夹或确认后覆盖原图；覆盖前会检测源文件变化，并通过同目录临时文件和 Windows 原子替换降低中断损坏风险。 |
| 隐私元数据可控 | 默认移除 EXIF 等隐私相关元数据并保留显示所需信息，也可切换为保留当前支持的元数据。 |
| GUI 与自动化共用核心 | 除桌面界面外还提供 JSON CLI 与 MCP stdio Agent；目录需显式授权，覆盖原图需要进程参数和请求字段双重许可。 |
| 本地化界面 | 支持简体中文与 English，以及跟随系统、明亮、深色主题；常用设置会保存在本机。 |

## GUI、CLI 与 MCP

桌面 GUI 面向日常批处理，提供拖放、队列、预览、失败重试和输出位置操作。Agent 使用同一个
`image-slim-core`，适合脚本和 AI 工具自动化；它不启动 GUI、不监听网络端口，也不会返回图片字节。

```powershell
# 查询能力；不需要目录权限
image-slim-agent.exe capabilities --json

# 从 stdin 读取计划请求；CLI plan 不创建跨进程 plan_id
'{"request_id":"11111111-1111-4111-8111-111111111111","paths":["D:\\Pictures"]}' |
  image-slim-agent.exe --allow-root D:\Pictures plan --request -

# MCP stdio；status 与 cancel 只在这个持久进程中提供
image-slim-agent.exe --allow-root D:\Pictures mcp
```

MCP 暴露 `image_slim_capabilities`、`image_slim_plan`、`image_slim_compress`、
`image_slim_status` 和 `image_slim_cancel`。读取必须位于显式 `--allow-root` 内；覆盖原图还需要
进程参数 `--allow-overwrite` 与请求字段双重授权。完整协议、Codex 与 Claude 配置示例见
[`docs/agent-protocol.zh-CN.md`](docs/agent-protocol.zh-CN.md)。

## 支持范围

| 格式 | 支持 | 明确不支持 |
|---|---|---|
| PNG | 静态 8/16 位 PNG、索引色、透明通道 | APNG |
| JPEG | RGB/灰度、基线/渐进 JPEG | CMYK/YCCK JPEG |
| WebP | 静态 VP8/VP8L WebP、透明通道 | 动画 WebP |

扩展名与文件魔数不一致、文件损坏和符号链接会逐项报错并跳过。输入边界如下；等于边界时允许加入，
超限项不会中断队列中的其他文件：

- 单文件最大 `512 MiB`（`536,870,912` 字节）。
- 单图最大 `100,000,000` 像素，任一边最大 `65,535` 像素。
- 单次队列最多 `10,000` 张图片，达到上限后停止遍历剩余目录。
- 处理前会按格式估算峰值内存并为系统/WebView 保留内存；即使格式边界允许，当前可用内存不足时仍会明确拒绝。

首版不包含 AVIF、格式转换、缩放、GIF、目标 KB 压缩或图片编辑。

## 压缩档位

| 档位 | PNG | JPEG | WebP |
|---|---|---|---|
| 无损 | OxiPNG 无损优化，并验证完整解码像素 | 保留 DCT 系数，仅优化编码和渐进扫描 | 清理容器；VP8L 额外尝试 exact 无损重编码 |
| 均衡 | libimagequant 中等质量量化，再经 OxiPNG | MozJPEG quality 82、4:2:0、渐进与 trellis | libwebp quality 80、method 6、sharp YUV |
| 强力 | 更低颜色预算与更慢量化，再经 OxiPNG | MozJPEG quality 68，其余优化同均衡档 | libwebp quality 65、method 6、较低 alpha quality |

所有候选结果都会重新解码，验证格式、尺寸和完整性。无损档还会比较解码像素；
PNG 校验包含完全透明像素下隐藏的 RGB。应用只在候选文件更小时采用结果。

## 输出安全

子文件夹模式默认写入输入根目录下的 `compressed`，并保持原有相对路径。
覆盖模式不会创建 `.bak`，因此启动前会对整批任务确认一次。

覆盖流程包含以下保护：

1. 扫描时规范化并去重路径，不跟随符号链接，也不重复扫描输出目录。
2. 压缩后先在目标文件同目录写入唯一临时文件并同步到磁盘。
3. 替换前重新核对源文件路径、大小、修改时间和完整字节内容。
4. 再次检查取消状态与输出路径边界，然后执行 Windows 原子替换。
5. 如果候选结果没有更小，覆盖模式保持原文件不变，子文件夹模式复制原图。

## 使用方式

1. 使用工具栏选择图片/文件夹，或直接拖入窗口。
2. 选择压缩档位、输出模式与元数据策略。
3. 在任务列表中选择图片检查预览；可同步缩放并拖动对比滑杆。
4. 点击“开始压缩”。已有输出或覆盖原图时，应用会先显示一次确认。
5. 完成后可从任务行直接打开输出位置；失败项可以单独重试。

## 从源码运行

### 环境要求

- Windows 10/11 x64 与 Microsoft Edge WebView2 Runtime。
- Node.js 24、npm 11（当前已验证版本）。
- Rust `1.93+`；当前发布构建使用 Rust `1.96`。
- Visual Studio 2022 Build Tools，包含 MSVC C++ 工具链和 Windows SDK。

```powershell
npm ci
npm run tauri:dev
```

中国大陆网络环境可使用：

```powershell
npm ci --registry=https://registry.npmmirror.com/
```

## 验证

```powershell
npm run check
npm test
npm run build
npm run ipc:check
npm run version:check
npm run config:check

Push-Location src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo check --workspace --all-targets --locked
Pop-Location
```

当前测试覆盖真实 PNG/JPEG/WebP 编解码、16 位 PNG、透明像素、输入边界、分批扫描、去重与容量、
内存调度、预览缓存与取消、虚拟队列、设置迁移、元数据清理、无收益回退、BLAKE3 源文件变更检测
与受保护原子写入。

## 构建发布产物

构建 Windows x64 NSIS 安装包和便携版，并自动生成许可汇编与校验文件：

```powershell
npm run tauri:build
```

最终产物统一放在项目根目录 `release/`，包括版本化安装包、GUI 便携 EXE、Agent EXE、许可文件和
`SHA256SUMS.txt`。只构建两个独立 EXE、不生成安装包时使用：

```powershell
npm run tauri:build:no-bundle
```

`src-tauri/target/` 仅作为编译缓存和中间目录。`0.1.0` 产物尚未代码签名，Windows
SmartScreen 可能显示未知发布者提示。

静态下载页位于 `site/`，合并到 `main` 后由 [Pages workflow](.github/workflows/pages.yml)
自动部署到 <https://t-meow.github.io/image-slim/>。

## 项目结构

```text
src/                    Svelte 界面、状态与 Tauri IPC 封装
src-tauri/src/          Tauri 命令、窗口状态与 GUI 事件 adapter
src-tauri/crates/       共享 core 与 Agent/CLI/MCP crate
src-tauri/capabilities/ Tauri 权限边界
scripts/                配置、版本与第三方许可检查脚本
docs/                   中文实施与构建说明
release/                本机构建的版本化发布产物（Git 忽略）
```

编解码器版本、静态链接方式与可复现构建说明见
[`docs/codec-build.zh-CN.md`](docs/codec-build.zh-CN.md)。
12MP/48MP 三格式三档的 release 性能记录见
[`docs/performance-baseline.zh-CN.md`](docs/performance-baseline.zh-CN.md)。
Agent 的权限、JSON CLI、MCP 工具与宿主配置示例见
[`docs/agent-protocol.zh-CN.md`](docs/agent-protocol.zh-CN.md)。

## 隐私与网络

应用运行时不发起网络请求，不包含云同步、账号、遥测或自动更新。图片内容、文件路径、
预览缓存和压缩结果都保留在本机；预览缓存会在应用下次启动时清理。

## 参与贡献

欢迎提交问题和 Pull Request。提交代码前请运行“验证”一节中的全部命令，并保持 Windows x64、
静态 PNG/JPEG/WebP 与既有输出安全语义兼容。涉及新格式、大型依赖或网络功能的改动，请先说明
体积、许可、隐私与失败处理方案。

## 许可

image-slim 以 [GPL-3.0-or-later](LICENSE) 发布，原因之一是 PNG 量化引擎
libimagequant 使用 GPL-3.0-or-later。第三方归属与完整许可文本见
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) 和
[`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt)。
