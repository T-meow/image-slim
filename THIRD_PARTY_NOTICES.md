# 第三方许可说明

本项目以 GPL-3.0-or-later 发布。发布构建以 `package-lock.json` 与
`src-tauri/Cargo.lock` 为唯一版本依据，并随安装包附带自动生成的
`THIRD_PARTY_LICENSES.txt` 完整许可汇编。

主要编解码依赖如下：

| 组件 | 锁定版本 | 许可 | 用途 |
|---|---:|---|---|
| imagequant / libimagequant | 4.4.1 | GPL-3.0-or-later | PNG 感知量化 |
| OxiPNG | 10.1.1 | MIT | PNG 无损优化 |
| mozjpeg | 0.10.13 | IJG | JPEG 有损编码 |
| mozjpeg-sys | 2.2.3 | IJG AND Zlib AND BSD-3-Clause | MozJPEG 静态链接与 DCT 转码 API |
| webpx | 0.4.0 | MIT OR Apache-2.0 | WebP 编解码与容器元数据 |
| libwebp-sys | 0.14.4 | MIT；其捆绑 libwebp 为 BSD 风格许可 | libwebp 静态链接 |
| img-parts | 0.4.0 | MIT OR Apache-2.0 | PNG/JPEG 元数据容器处理 |
| image | 0.25.10 | MIT OR Apache-2.0 | 解码验证 |
| BLAKE3 | 1.8.5 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | 流式源文件与缓存内容校验 |

JPEG 无损转码代码改编自 MIT 许可的 `losslessly 0.1.1`，其原始许可与归属
已单独写入生成的许可汇编。当前构建不包含外置 `jpegtran.exe`；相同的
MozJPEG DCT 系数转码 API 被静态链接到应用进程，以避免额外 sidecar 与进程调用面。

完整依赖、著作权声明及许可原文使用以下命令从本机锁定依赖重新生成：

```powershell
npm run licenses
```
