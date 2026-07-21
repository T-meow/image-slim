# image-slim 0.1.0

image-slim 的首个 Windows x64 版本。图片压缩、预览和自动化全部在本机完成，不需要账号，
不上传图片，也不包含遥测。

## 本次发布

- 桌面 GUI 支持递归扫描 PNG、JPEG、WebP，提供无损、均衡、强力三档压缩。
- 支持批量队列、取消、失败重试、统计、前后对比预览与中英文界面。
- 提供 `compressed` 子目录与覆盖原图两种输出模式；覆盖流程包含源变更检测、临时文件和原子替换保护。
- 默认清理隐私相关元数据，并在候选文件没有更小时保留原图。
- 安装包内含 `image-slim-agent.exe`，同时支持同步 JSON CLI 与五个 MCP stdio 工具。
- Agent 使用显式允许根目录、覆盖双重授权、幂等请求、计划内容哈希和有界任务缓存。

## 下载选择

- `image-slim_0.1.0_x64-setup.exe`：推荐，安装 GUI 与 Agent。
- `image-slim_0.1.0_x64-portable.exe`：免安装 GUI 单文件。
- `image-slim-agent_0.1.0_x64.exe`：独立 Agent，适合 CLI/MCP 自动化。
- `SHA256SUMS.txt`：所有发布文件的 SHA-256 校验值。

当前产物尚未代码签名，Windows SmartScreen 可能显示未知发布者。项目运行时完全离线；下载和
GitHub Pages 仅用于分发程序与文档。
