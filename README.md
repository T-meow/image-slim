<p align="center">
  <img src="assets/icon.svg" width="88" alt="image-slim 图标">
</p>

<h1 align="center">image-slim</h1>

<p align="center">完全离线的 Windows 批量图片压缩器。</p>

<p align="center">
  <strong>简体中文</strong> · <a href="README.en.md">English</a>
</p>

image-slim 使用 Tauri 2、Svelte 5 与 Rust 构建，在本机压缩 PNG、JPEG 和 WebP，
不上传图片，不需要账号，也不包含遥测。当前版本为 `0.1.0`，仅支持 Windows 10/11 x64。

## 主要功能

- 拖入多个文件、文件夹或混合输入，递归扫描并保留相对目录结构。
- 提供无损、均衡、强力三个档位；压缩结果不更小时保留原始文件。
- 支持批量队列、失败重试、取消、输出统计和前后滑杆对比预览。
- 可输出到可编辑的 `compressed` 子文件夹，或经整批确认后覆盖原图。
- 覆盖前检查源文件是否被外部修改，并使用同目录临时文件完成原子替换。
- 默认清理隐私相关元数据并保留显示必需信息，也可切换为保留已支持的元数据。
- 支持简体中文/English、跟随系统/明亮/深色主题，设置会保存在本机。

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
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo check --all-targets --locked
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

最终产物统一放在项目根目录 `release/`，包括版本化安装包、便携 EXE、许可文件和
`SHA256SUMS.txt`。只构建便携版时使用：

```powershell
npm run tauri:build:no-bundle
```

`src-tauri/target/` 仅作为编译缓存和中间目录。`0.1.0` 产物尚未代码签名，Windows
SmartScreen 可能显示未知发布者提示。

## 项目结构

```text
src/                    Svelte 界面、状态与 Tauri IPC 封装
src-tauri/src/          Rust 扫描器、编解码器、批处理与原子输出
src-tauri/capabilities/ Tauri 权限边界
scripts/                配置、版本与第三方许可检查脚本
docs/                   中文实施与构建说明
release/                本机构建的版本化发布产物（Git 忽略）
```

编解码器版本、静态链接方式与可复现构建说明见
[`docs/codec-build.zh-CN.md`](docs/codec-build.zh-CN.md)。
12MP/48MP 三格式三档的 release 性能记录见
[`docs/performance-baseline.zh-CN.md`](docs/performance-baseline.zh-CN.md)。

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
