# 音频压缩实现记录

## 目标与授权

- 2026-09-12：用户要求额外加入音频压缩，并明确选择“MP3，兼容性优先”。实现本地离线音频导入、码率设置、批量压缩及试听，完成必要回归并重新打包供验收。
- 单助手；沿用当前模型与推理设置；不使用 Computer Use。实现阶段不提交 Git、不发布；随后用户明确要求“提交github并发布”，授权将当前界面重构与音频功能一并提交、推送并发布 GitHub Release。保留所有既有改动。
- 以当前工作区为基线；先前 UI 重构记录中的 Rust/锁文件禁动范围仅适用于旧任务。本次音频功能授权覆盖必要的 Rust 核心、IPC、音频依赖及锁文件、界面和打包许可说明。

## 决定

- 采用内置 Symphonia 解码与 LAME MP3 编码，维持单文件便携版与离线运行，不引入外部 FFmpeg 运行依赖。
- 首版输入 MP3、WAV、FLAC、M4A（AAC-LC/ALAC）和 Ogg Vorbis；输出 MP3，提供 192/128/64 kbps 档位，默认 128 kbps。只处理单声道/双声道音频；损坏、其他编码或多声道文件逐项报错。
- 复用扫描、批处理、取消、源文件校验与原子输出。音频始终输出到子文件夹，保留源文件；结果不更小时保留原文件，转换时不将原格式字节写入 `.mp3`。
- 音频独立设置码率，不使用图片的“无损”档位；移除音频标签及封面。选择音频时展示信息与原文件/已保存结果试听，不生成图片预览。

## 进度与验证

- 已核对 Git 状态、当前任务记录、扫描/批处理/输出保护、IPC 与前端入口；已确认 MP3 输出偏好。
- 已实现：内置音频编解码、MP3/WAV/FLAC/M4A/Ogg 识别、音频信息与独立码率、混合队列、受保护子文件夹输出和试听；图片预览绕过音频。Rust 全目标检查及前端检查均通过。
- 真实音频回归：自生成 7 个样本（包含单/双声道 WAV、MP3、FLAC、AAC/ALAC M4A、Ogg Vorbis），7 项音频回归通过；覆盖三种码率、损坏/取消、保留源文件、冲突确认、无收益不输出、重复目标及同批次源文件保护。
- 已修正 M4A 仅在解码后提供声道参数的兼容问题，保留探测时解码的首包样本。音频按分包处理所需工作集估算内存，并预留至少 1 GiB 及当前可用物理内存的一半；音频单元测试使用现有测试预算注入，图片内存策略保持原有行为。
- 检查通过：前端 33 项、核心 45 项（另有 1 项手动性能基准忽略）、Agent 单元 2 项及 CLI/MCP stdio 3 项；`npm run check` 无错误/警告，版本、配置、归档配置、生成 IPC 校验及全目标 Clippy 均通过。新增能力令 MCP 工具描述超过 16 KiB 后，已去掉数值格式注解并复用错误定义，保持原有大小上限与字段结构。
- 测试限制：全工作区测试中 2 项既有图片 Agent 集成测试未通过（另外 3 项通过）。独立 GUI 共用核心的 Agent 图片调用确认错误为 `insufficient_memory`，可用预算为 `0`，是当时主机可用物理内存低于图片调度器整机 20% 预留所致；诊断保存在 `src-tauri/target/audio-validation/image-memory-diagnostic.json`。没有修改图片的内存策略或关闭保护。
- Release 音频验证：实际执行优化构建的 Agent，对 7 个样本分别运行 192/128/64 kbps，生成 17 个更小的 MP3、4 次无收益保留；全部输出经独立 FFmpeg/FFprobe 解码与时长/声道核对，7 个源文件哈希保持不变。代表性音频信号相关性 `0.9999952377`。完整报告：`src-tauri/target/audio-validation/audio-roundtrip.json`。
- Windows GUI/NSIS 构建和归档完成；独立重算 `SHA256SUMS.txt` 全部 6 个文件的 SHA-256，一致。便携版与 Agent 的 PE 架构均为 x64；NSIS 启动器为 x86，内含 x64 应用。最终 Agent 哈希与上述音频实测二进制一致，并已执行归档版本 `capabilities --json` 确认三档 MP3 能力。`package-lock.json` 保持原哈希；`git diff --check` 通过。
- 已复查音频首包保留、完整解码、无收益处理、输出冲突和源文件保护、试听暂停及 MCP schema；未发现需追加修改的问题。归档验证报告：`src-tauri/target/audio-validation/release-verification.json`。
- 开发与打包已完成；未运行安装程序或操作图形界面，安装及实际播放器体验尚待手动验收。上述 2 项受主机内存限制的图片 Agent 集成测试未宣称通过。
- 依赖参考：[Symphonia](https://docs.rs/symphonia/0.5.5/symphonia/)、[mp3lame-encoder](https://docs.rs/mp3lame-encoder/0.2.5/mp3lame_encoder/)、[mp3lame-sys](https://docs.rs/crate/mp3lame-sys/0.1.11)。

## 依赖与许可

- 使用现有 Cargo 用户缓存 `C:\Users\Ferris\.cargo\registry`，锁文件新增 24 个包；压缩归档共 `2,581,032` 字节，展开源码共 `11,819,490` 字节。24 个 `.crate` 已独立重算 SHA-256 并与 `src-tauri/Cargo.lock` 的 checksum 逐一核对，全部一致；既有包的锁定校验值未改变，未新增 npm 依赖。
- 主要版本：Symphonia `0.5.5`、mp3lame-encoder `0.2.5`、mp3lame-sys `0.1.11`（静态 LAME `3.100`）。构建缓存沿用项目 `src-tauri/target/`。
- 已补齐 Symphonia 官方 MPL-2.0 文本、LAME 原生 `COPYING` 和绑定许可。`third-party/symphonia/MPL-2.0.txt` 为 `16,726` 字节，SHA-256 `3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04`；许可汇编通过现有 `npm run licenses` 生成。

## 最终产物

沿用版本 `0.1.0`，以下为本地包含音频功能的新构建，未发布到线上；许可文件及校验清单一并保存在 `D:\Projects\image-slim\release`。

| 文件 | 字节 / MiB | SHA-256 |
| --- | --- | --- |
| `image-slim_0.1.0_x64-portable.exe` | 11,730,944 / 11.19 | `137665d9487531668cbf2bb20edfad98f821599bb8b504323e65475985307890` |
| `image-slim_0.1.0_x64-setup.exe` | 5,560,153 / 5.30 | `0f53370bac21df5c8224cdf2f7aaa3491235dfeac1378d1aa10af9678dafcec9` |
| `image-slim-agent_0.1.0_x64.exe` | 8,048,640 / 7.68 | `eb0426479d3f3099fb9d5bccfe1c272c9be8c5bdbc740f524c045bfa1d51bf59` |

## 路径与禁动项

- 项目 `D:\Projects\image-slim`；修改 `src/`、`src-tauri/`、相关说明与许可；产物继续在 `release/`，中间输出在现有 `src-tauri/target/`。
- 保留现有图片编解码行为和目录授权；不覆盖用户改动、不修改账号/系统配置、不运行安装程序。

## GitHub 发布（2026-09-12）

- 目标与授权：用户明确要求“提交github并发布”。源为当前 `D:\Projects\image-slim` 工作区，目标为 `T-meow/image-slim` 的 `main` 与新 Release；包括此前尚未提交的界面重构及本次音频功能，安装包、便携版、Agent、许可与校验文件作为附件。
- 决定：现有线上版本是 `v0.1.0`；新功能发布为 `v0.2.0`，同步包清单、Rust workspace、README 与官网，重建正式版本二进制。保留旧发布和标签，不使用强制推送，不运行安装程序。
- 现场：本地 `main` 与远端起点同为 `5bfbb737cfe200e97be8296eeb248a95dbf65b7d`，仓库为公开仓库，当前 GitHub 身份有管理权限。
- 进度：`v0.2.0` 已完成 Windows GUI/Agent/NSIS 正式构建；版本、配置、前端检查及 App 的 4 项回归通过。独立核对 137 个构建输入在打包过程中未改变，7 个待上传附件的大小与 SHA-256 已记录；官网版本、下载链接、大小与能力示例均匹配实际归档。npm 锁文件除项目版本字段外与原基线一致，没有更新 npm 依赖。
- 正式版实测：归档 Agent 返回 `app_version: 0.2.0`，与实际 WAV/M4A → 128 kbps MP3 测试的二进制哈希一致；两项结果均更小，FFmpeg/FFprobe 解码、时长、声道和码率验证通过，源文件哈希未变。
- 发布说明：`docs/release-v0.2.0.zh-CN.md`。最终附件：`release/image-slim_0.2.0_x64-portable.exe`（11,730,944 字节，SHA-256 `310c49af37ceaa170e76d41a51ae960271f66c7a956f0409efc44d9e7074eed4`）、`release/image-slim_0.2.0_x64-setup.exe`（5,559,028 字节，`72a4f67f0039fb957f097e6e21a857e788019b52b6c18fc2be007754b77dae53`）、`release/image-slim-agent_0.2.0_x64.exe`（8,048,640 字节，`be8c0610ceb5053a68996a7b9a47f5c1f5232ce0d655c7eeeeb17b4cc8cb26c8`）。完整校验报告为 `src-tauri/target/audio-validation/v0.2.0-release-verification.json`。
- 提交与推送：`c239ad304b36eed5b2bd9816dbdd3d32fe684190` 已推送到 GitHub `main`。常规 Git HTTPS 因连接超时/重置失败后，使用 GitHub Git database API 上传对象，核对完整源码树 `ad31990e1cb84541339b9ecdc18bc2f9d5634905` 和 commit SHA 与本地完全一致，再以 `force: false` 更新分支；未改变提交或覆盖远端历史。传输记录：`src-tauri/target/audio-validation/v0.2.0-api-push.json`。
- 附件与官网：`v0.2.0` Release 草稿已创建，7 个附件均已上传并核对 GitHub digest 和大小；目标 commit 为上述提交。Pages 运行 `34700171520` 成功，在线官网 HTML 与本地 `site/index.html` 一致。
- CI 修正：首次 Quality checks（`34700171470`）在 Clippy 失败。日志确认虽然 workflow 安装 1.96.0，仓库 `rust-toolchain.toml` 的 `stable` 覆盖设置让检查实际运行 Rust 1.98，触发旧 PNG 代码的新 lint。给 workflow 显式设置 `RUSTUP_TOOLCHAIN: 1.96.0`，并在现有配置检查中验证它，让两项 CI 任务均使用已约定版本；保留全部 lint 和测试要求，不修改图片算法或本机工具链。正式附件继续使用已验证的本机 Rust 1.96 构建，应用源码未因本次 CI 修正改变。
- 修正提交：`5b7215534e2c163bcf6ed59629525b0f83e8e9da` 已通过同样的哈希校验与非强制分支更新推送，触发检查运行 `34700747386`。此提交仅包含 CI 配置、配置检查脚本及本记录，不改变发布二进制的应用源码。
- 第二轮检查：`34700747386` 的 Clippy、Agent 单元 2 项、服务集成 5 项、stdio 3 项均通过，包含此前本机因内存不足未通过的 2 项图片测试；核心 44 项通过、1 项忽略，1 项音频测试在比较 Windows 短路径 `RUNNER~1` 与长路径 `runneradmin` 时失败。修正该测试为比较规范化后的同一文件路径，保留源字节不变和不写入伪 MP3 的断言；应用处理结果正确，无需改变压缩逻辑。
- 官网收尾：根据 GitHub 实际渲染的 README 锚点修正 CLI 示例链接。
- 待办：推送测试与链接修正，更新草稿目标并等待完整 Windows 检查；发布 Release、核对公开标签/附件和 latest 指向，提交发布结果记录。
