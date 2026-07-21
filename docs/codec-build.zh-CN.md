# 编解码器与发布构建

## 固定版本

Windows x64 发布构建以 `src-tauri/Cargo.lock` 固定 OxiPNG、libimagequant、
MozJPEG 与 libwebp 版本。`mozjpeg-sys` 和 `libwebp-sys` 在 Cargo 构建中编译并
静态链接其原生实现，因此安装后不依赖系统编解码器，也不访问网络。

JPEG 无损档直接调用 MozJPEG 的系数读取、Huffman 优化和渐进扫描 API；该路径
不重新量化 DCT 系数。它替代了方案草案中的外置 `jpegtran.exe`，结果语义一致，
同时减少一个可执行 sidecar、一次进程启动和对应的路径授权面。

## 可复现构建

```powershell
npm ci --registry=https://registry.npmmirror.com/
npm run licenses
npm run check
npm test
Push-Location src-tauri
cargo test --locked
cargo check --all-targets --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
Pop-Location
npm run tauri:build
```

输出目标固定为 NSIS。当前首版不签名，也不生成 MSI。安装包中应包含完整 GPLv3
文本、`THIRD_PARTY_NOTICES.md` 和 `THIRD_PARTY_LICENSES.txt`。

需要单文件便携版时，使用 `npm run tauri:build:no-bundle`。脚本会将版本化 EXE、
完整 GPLv3、第三方许可文件与 `SHA256SUMS.txt` 统一归档到项目根目录 `release/`。
`src-tauri/target/` 只保留为编译缓存和中间输出目录。
