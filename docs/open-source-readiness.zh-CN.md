# image-slim 开源发布准备检查

检查日期：2026-07-16

## 已完成

- 中英文 README 已重写并互相链接，内容与当前 `0.1.0` 实现一致。
- 项目使用完整 GPLv3 文本，并维护第三方许可说明与锁定依赖许可汇编。
- `package-lock.json` 与 `src-tauri/Cargo.lock` 已固定依赖版本。
- Windows x64 NSIS 与便携 EXE 已完成本机 release 启动烟测。
- 前端检查、前端测试、Rust 测试、rustfmt、严格 Clippy 与 release build 已通过。
- 窄范围仓库检查未发现明显密钥、私钥、密码或本机绝对路径泄漏。
- README 中所有本地链接目标存在，中文 UTF-8 读取正常。

## P0：公开仓库前处理

### 1. 初始化仓库并先建立忽略规则

当前目录不是 Git 仓库，也没有 `.gitignore` 或 `.gitattributes`。初始化前必须排除：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- `portable/`
- 本地日志、编辑器目录、临时文件和系统文件

否则第一次提交可能包含依赖缓存、数 GB Rust 构建产物和发布二进制。

### 2. 确定公开身份与应用标识

当前 `local.imageslim.desktop` 是临时标识，Cargo 作者仍是 `image-slim contributors`，
`package.json` 也没有 `description`、`repository`、`homepage`、`bugs`、`author` 和 `engines`。

需要先确定 GitHub 用户/组织、仓库 URL、维护者署名和长期使用的反向域名标识，再统一更新：

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- README 的 Issues、Releases 与源码链接

`package.json` 的 `private: true` 可以保留，它只阻止误发布到 npm，不影响 GitHub 开源。

### 3. 分离生产与开发 CSP

当前生产 `csp` 仍允许 `127.0.0.1:1421` 与对应 WebSocket。应将 Vite 开发地址移动到
Tauri 的 `app.security.devCsp`，生产 `csp` 仅保留应用自身资源、asset protocol 与 IPC。

验收时需要确认 release 版本不能加载本机开发服务器内容，同时拖入图片和预览仍正常。

### 4. 处理 npm 安全公告

2026-07-16 使用 npm 官方审计端点检查时发现 1 项低危问题：

- `esbuild 0.27.7`：Windows 开发服务器可能被利用读取任意文件。
- 公告：`GHSA-g7r4-m6w7-qqqr`。
- 已确认 `esbuild 0.28.1` 不在公告所列受影响范围内，且符合 Vite 8 的 peer 范围。

升级后应重新生成 `package-lock.json`，运行全部前端检查、Tauri 开发启动与 release 构建。

### 5. 校准“保留支持的元数据”的产品语义

当前无损 JPEG 可以复制完整 JPEG marker；PNG 有损路径恢复选定的常见 ancillary chunk，
JPEG 有损路径主要恢复 ICC/EXIF，WebP 恢复 ICC/EXIF/XMP。项目已选择可验证的有限契约：
中英文界面统一使用“保留支持的元数据 / Keep supported metadata”，协议值使用 `supported`，
并自动迁移旧 localStorage 中的 `all` 值。

### 6. 完成名称与许可发布核查

- 核查 `image-slim` 名称、图标与应用标识在目标发布地区的商标和项目重名风险。
- 每个二进制 Release 必须对应一个可获取的源代码 tag/归档，并附 GPL 与第三方许可文件。
- 保留改编自 `losslessly 0.1.1` 的 MIT 归属说明。
- 在 Release 页面明确首版未签名及 SmartScreen 提示，提供 SHA-256 校验文件。

## P1：首个正式 Release 前完成

### 1. 建立 Windows CI

建议 GitHub Actions 至少执行：

```powershell
npm ci
npm run check
npm test
Push-Location src-tauri
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
Pop-Location
npm run tauri:build:no-bundle
```

构建脚本会将许可汇编、NSIS、便携版和 `SHA256SUMS.txt` 统一归档到根目录 `release/`；
该目录由 Git 忽略，Release 工作流应直接上传其中经过校验的文件。

### 2. 增加仓库治理文件

- `CONTRIBUTING.md`：环境、测试、提交与 PR 要求。
- `SECURITY.md`：私下报告覆盖/路径安全问题的渠道与支持版本。
- `CODE_OF_CONDUCT.md`：贡献者行为准则。
- `CHANGELOG.md`：从 `0.1.0` 开始记录用户可见变更。
- `.github/ISSUE_TEMPLATE/` 与 Pull Request 模板。

### 3. 补齐固定图片语料与集成测试

当前 Rust 有 18 项测试、前端有 3 项辅助函数测试，但尚无独立固定语料目录。应加入许可清晰、
体积可控的测试图片，覆盖：

- 隐藏透明 RGB、索引色和不可降位的 16 位 PNG。
- 灰度/渐进 JPEG、EXIF 方向、ICC、CMYK 拒绝样本。
- VP8/VP8L、透明 WebP、动画拒绝样本。
- Unicode/长路径、只读目标、冲突、取消与异常临时文件恢复。

如果继续承诺 PNG 原生位深不变，还需增加输入/输出 PNG bit depth 的结构级断言；当前主要验证
解码像素一致。

### 4. 增加真实界面素材

README 尚无界面截图。公开前建议加入一张明亮主题和一张深色主题的真实应用截图，内容使用
可公开的测试图片，不包含用户名、绝对路径或私人文件名。

### 5. 完成 Rust 依赖公告检查

当前未安装 `cargo-audit`，尚未查询 RustSec 公告库。安装或在 CI 中运行后，应记录处置结果；
这不替代代码安全审查。

## P2：发布质量增强

- 评估 Windows 代码签名，降低 SmartScreen 对未知发布者的拦截。
- 配置 Dependabot 或 Renovate，但合并升级前仍运行真实编解码测试。
- 增加前端组件状态测试和关键拖拽/批处理流程测试。
- 将便携目录复制、许可文件和校验和生成固化为一个可复现脚本。
- 统一后端错误码与中英文用户提示，减少直接暴露英文库错误。
- 根据用户反馈评估单实例、崩溃后恢复和更多格式；不要默认引入联网更新或遥测。

## 本次检查边界

本次执行的是发布卫生与依赖公告的窄范围检查，不是深度安全扫描，也没有进行商标法律意见、
签名证书采购、GitHub 发布或远端仓库写入。
