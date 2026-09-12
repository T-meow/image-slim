# UI 与批处理流程重构记录

## 目标与授权

- 2026-09-12：用户在只读评估后要求“进行一下重构”。实施第一轮：预览修正、设置与结果分离、队列筛选及完成摘要，并修复重试丢行。
- 2026-09-12：用户要求“打包exe我来检查”，授权本地 Windows EXE 构建与归档，供用户手动验收。
- 单助手；不使用 Computer Use，不提交 Git，不发布。基线 `5bfbb73`，开始时工作区干净，无需保留的未跟踪文件。

## 决定

- 沿用 Svelte、现有 IPC 和 Rust 压缩/输出安全语义，不引入新依赖。
- 将扫描、重试、启动确认、事件缓冲和取消协调移入前端控制器；启动期间锁定操作。
- 设置用于下一次运行，已完成结果保留；仅在新批次获准启动后替换对应任务状态。
- 重试先扫描，成功项再运行；失败或取消扫描保留原任务。完成项允许显式重新处理。
- 修正左右图层，支持画布分界线拖动与键盘操作。现有预览最大边为 2048；标明缩略预览，缩放以适应窗口为基准，不声称原图像素级检查。
- 搜索、状态筛选保留虚拟列表；完成摘要采用当前队列的累积结果。

## 进度与验证

- 已完成 `WorkflowController`：扫描、重试、启动确认、提前到达的事件、取消统一协调；主界面由 601 行缩减至 478 行。
- 设置不再重置结果；每次正式启动记录设置快照。重试在扫描及覆盖确认阶段保留旧任务，成功启动后才更新对应行。覆盖成功后展示已保存文件。
- 已修正左右图层、直接拖动与键盘调整、适应窗口及 0.5×–3× 视图缩放；明确提示缩略预览。
- 已增加搜索、五种状态筛选、路径/体积/节省排序、队列累计摘要、仅重试失败、查看结果和清除已完成；修复深滚动后筛选少量结果时的虚拟列表边界。
- 已同步中英文文案和 README，读回修改片段并复查差异及启动/重试/取消逻辑。
- 验证通过：`npm run check`（0 错误、0 警告）、`npm test`（10 文件、32 项通过）、`npm run build`、`npm run version:check`、`npm run config:check`、`git diff --check`。
- 回归覆盖设置变化保留结果、覆盖后停止旧源预览、扫描失败/取消不丢行、取消覆盖确认保留旧结果、队列满额重试、目录映射、重复启动/导入锁定、提前/迟到事件、设置快照、筛选及拖动。
- 前端构建输出：`dist/`；CSS 18.40 kB，JS 149.73 kB。本轮没有 Rust/IPC 逻辑变更；后续已完成 Windows EXE 构建，桌面视觉及原生文件交互由用户手动验收。

## Windows EXE 打包

- 2026-09-12：`npm run tauri:build` 成功，沿用 Rust/Cargo `1.96.0`、Windows x64 MSVC 工具链及项目 NSIS 配置。
- 构建包含 Agent、许可汇编、前端资源、桌面主程序及 NSIS 安装包；没有发布或运行安装程序。
- 产物目录：`D:\Projects\image-slim\release`。便携版可直接运行，无需安装。

| 文件 | 字节 | SHA-256 |
|---|---:|---|
| `image-slim_0.1.0_x64-portable.exe` | 9,925,120 | `e3735da70c01cb59bd0200e2df60b6b6c8d06ae7a6fe17a8bfaff46bc8b94cb3` |
| `image-slim_0.1.0_x64-setup.exe` | 4,566,989 | `b894749eebfabc8c2bfe2f05e0878de19c2d3debbc38c475b9a62ab260608634` |
| `image-slim-agent_0.1.0_x64.exe` | 6,267,392 | `9e12a53b7143376a0ec6bf1e45e14e0e5b4c2336a3e017cca4ec239acae013da` |

- `release/SHA256SUMS.txt` 中 6 个产物已独立重新计算并核对 SHA-256，全部通过；3 个 EXE 的 PE 文件头有效，GUI/Agent 为 x64。
- 归档脚本的文件体积上限检查及复制前后哈希校验通过。重新生成的 Cargo 配置、schema 和许可汇编没有 Git 内容差异；锁文件未改变。

## 最新软件重新打包（2026-09-12）

- 目标与授权：用户要求“image slim打包最新软件”，基于当前工作区（含尚未提交的界面重构）重新生成 Windows x64 便携版、Agent 和 NSIS 安装包。
- 决定：沿用 `0.1.0` 版本、现有依赖与 `npm run tauri:build`，产物归档到项目 `release/`；保留用户已有改动，不提交、不发布、不运行安装程序，不使用 Computer Use。
- 进度与验证：2026-09-12 18:16（Asia/Shanghai）完成 `npm run tauri:build`，产物包含 Agent、许可文件、最新前端、桌面程序及 NSIS 安装包。`npm run version:check`、`npm run config:check`、`npm run release:check`、`npm run check`（0 错误、0 警告）和 `npm test`（10 文件、32 项）全部通过。
- 验收结果：独立重算并核对 `release/SHA256SUMS.txt` 中全部 6 个文件哈希；3 个 EXE 的 PE 文件头有效，主程序与 Agent 均为 x64，NSIS 使用 x86 安装引导程序。归档后的 Agent 执行 `capabilities --json` 成功，协议版本为 `1`，应用版本为 `0.1.0`。
- 源码保护：打包前后 69 个相关源码、脚本、配置及锁文件的聚合 SHA-256 均为 `f1cee939bcac12388668b4f6cbfd836354e0a91652c1f3940b71a332a22ae7cc`；Git 状态与原有改动一致，`git diff --check` 通过。沿用 Node.js `v24.19.0`、Rust/Cargo `1.96.0`，没有安装依赖。
- 待办：本次打包与文件校验已完成；图形界面及安装流程由用户手动验收。
- 路径与禁动项：沿用下方约束；本次仅更新任务记录与构建产物，不修改业务代码或锁文件。

| 本次产物（`release/`） | 字节 | SHA-256 |
|---|---:|---|
| `image-slim_0.1.0_x64-portable.exe` | 9,925,120 | `73b6c2b1d999feeee60d9db1aa00ca6a1e8cc5964eb8e938a7fef42a8450142e` |
| `image-slim_0.1.0_x64-setup.exe` | 4,565,887 | `ef8b1e11db079cc62a68ae3aac1719f831e41cf96c54a098e49893ed0a450e26` |
| `image-slim-agent_0.1.0_x64.exe` | 6,267,392 | `9e12a53b7143376a0ec6bf1e45e14e0e5b4c2336a3e017cca4ec239acae013da` |

## 依赖安装

- 使用现有锁文件执行 `npm ci --registry=https://registry.npmmirror.com/ --cache ./node_modules/.cache/npm --no-audit --no-fund`，安装 137 个包；无新依赖、锁文件无修改。
- 环境：Node.js `v24.19.0`。
- 安装路径：`D:\Projects\image-slim\node_modules`，共 `198,054,716` 字节（约 `188.9 MiB`，含缓存）。
- 下载缓存：`D:\Projects\image-slim\node_modules\.cache\npm`，`39,001,767` 字节（约 `37.2 MiB`）。
- `npm ci` 按锁文件 integrity 校验安装包。`package-lock.json` SHA-256：`ca12b6674a269eb612d051077db38c4fa6f1c89fdf5f93edb973153ef89b33b4`。

## 待办

- [x] 控制器与结果状态
- [x] 预览、队列、完成摘要及中英文文案
- [x] 必要回归、构建、读回与逻辑复查
- [x] Windows 安装版/便携版打包及产物校验；沿用 `npm run tauri:build`。
- 后续独立迭代：原图像素级/区域预览、可调两栏、输出目录与冲突策略、目标体积。未纳入本轮实现。

## 路径与禁动项

- 修改范围：`src/`、本记录及必要的使用说明。
- 依赖与构建产物：项目 `node_modules/`、`dist/`、`src-tauri/target/`、`src-tauri/binaries/`、`release/`；沿用 Rust 用户工具链及缓存，不修改锁文件或安装全局 npm 包。
- 不改 Rust 编解码器、输出权限、原子写入、Agent 协议及发布配置。
