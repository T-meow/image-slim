# image-slim 实施计划

## 目标

构建一个完全离线的 Windows 图片压缩工具。首版支持静态 PNG、JPEG 和 WebP，提供严格无损、均衡、强力三个档位，并支持文件/文件夹拖拽、批量队列、前后预览、子文件夹输出和受保护的原图覆盖。

## 完成状态（2026-07-15）

- 已完成 Tauri 2、Svelte 5、TypeScript 与 Rust 工程及中英双语明暗主题界面。
- 已完成文件/文件夹拖拽、递归扫描、路径去重、输出目录排除、批量队列和对比预览。
- 已完成 PNG、JPEG、WebP 三档压缩、解码校验、元数据策略、无收益回退和 2 路批处理。
- 已完成取消、冲突确认、源文件变更检测、路径越界防护和 Windows 原子替换。
- 已生成完整 GPLv3 与锁定依赖许可汇编，并随 NSIS 安装包打包。
- 已生成 NSIS 安装包与按后续要求追加的单文件便携 EXE；未生成 MSI，首版不签名。

## 范围与非目标

- 技术栈：Tauri 2、Svelte 5、TypeScript、Rust。
- 首发平台：Windows 10/11 x64。
- 输入格式：静态 PNG、RGB/灰度 JPEG、静态 WebP。
- 不包含格式转换、缩放、AVIF、GIF/APNG/动画 WebP、云服务、账号、跨启动历史和自动更新。
- 项目采用 GPL-3.0-or-later，第三方编解码器版本与许可在发布前固定并登记。

## 实施阶段

1. 建立 Svelte/Tauri 工程、双语文案、主题令牌与基础应用壳。
2. 实现路径扫描、格式探测、队列模型、拖拽和目录映射。
3. 接入 PNG、JPEG、WebP 三档编解码器，补齐元数据与输出校验。
4. 实现缓存预览、批处理进度、取消、重试和原子输出。
5. 完成单元测试、固定图片语料、构建检查和 Windows NSIS 打包。已完成。

## 数据与兼容风险

- 覆盖模式必须先写同目录临时文件，完成解码校验和源文件变更检查后再替换。
- 无损模式以原生位深解码结果一致为准，包含完全透明像素下的 RGB；结果不更小时保留原文件。
- 默认保留影响显示的 ICC、gamma/sRGB 和 EXIF 方向，其他元数据清理；用户可切换为保留编解码器支持的元数据。
- 不跟随符号链接，不处理输出根目录，扩展名与文件魔数不一致时拒绝处理。
- 动图、CMYK JPEG、超出 1 亿像素的图片和损坏文件逐项失败，不中断整个批次。

## 验证计划

- 前端：类型检查、组件测试、中英文布局、明暗主题、拖拽与核心状态测试。
- Rust：扫描、路径映射、格式探测、元数据策略、取消、原子替换和各编解码器测试。
- 语料：透明 PNG、索引/16 位 PNG、渐变、照片、灰度/渐进 JPEG、方向/ICC、VP8/VP8L、损坏及伪扩展文件。
- 构建：`npm run check`、`npm test`、`cargo test`、`cargo clippy -- -D warnings`、`npm run tauri:build`。

## 默认值

- 档位：均衡。
- 输出：`compressed` 子文件夹。
- 元数据：仅保留显示必需项。
- 主题与语言：首次跟随系统，之后记住用户选择。
- 并发：2；单图最大像素数：200,000,000。

## 实际验证记录

- `npm run check`：0 errors，0 warnings。
- `npm test`：3 项通过。
- `cargo test --locked`：18 项通过。
- `cargo check --all-targets --locked`：通过且无编译警告。
- `cargo fmt --check`：通过。
- `cargo clippy --all-targets --locked -- -D warnings`：通过。
- Tauri debug 与最终 release 可执行文件均完成窗口启动烟测。
- `npm run tauri:build`：成功生成单一 Windows x64 NSIS 安装包。
- `tauri build --no-bundle`：成功生成便携 release；从独立目录启动后窗口已创建且进程响应正常。
