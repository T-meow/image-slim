# image-slim Agent 使用与协议说明

`image-slim-agent.exe` 是完全离线的 Windows x64 控制台程序，提供一次性 JSON CLI 和 MCP stdio。
它不启动 GUI、不监听网络端口、不访问远端 API，也不会把图片字节、缩略图或逐文件成功记录返回给
调用方。

## 访问边界

扫描和压缩前必须通过可重复的 `--allow-root` 指定允许目录。未指定时只能读取 capabilities。
覆盖原图还需要同时提供进程参数 `--allow-overwrite` 和请求字段
`"output_mode":"overwrite"`；默认输出到输入根目录下的 `compressed` 子目录。

```powershell
image-slim-agent.exe capabilities --json
image-slim-agent.exe --allow-root D:\Pictures plan --request -
image-slim-agent.exe --allow-root D:\Pictures compress --request -
image-slim-agent.exe --allow-root D:\Pictures mcp
```

`--request -` 从 stdin 读取一个 UTF-8 JSON 对象。CLI stdout 只输出一行 JSON，MCP stdout 只输出
协议帧；诊断信息写入 stderr。

## JSON CLI

CLI `plan` 是一次性只读扫描，不返回可跨进程复用的 `plan_id`：

```powershell
@'
{"request_id":"11111111-1111-4111-8111-111111111111","paths":["D:\\Pictures"]}
'@ | image-slim-agent.exe --allow-root D:\Pictures plan --request -
```

CLI `compress` 只接受 `paths`，并等待任务完成后返回最终汇总：

```powershell
@'
{"request_id":"22222222-2222-4222-8222-222222222222","paths":["D:\\Pictures"],"preset":"balanced"}
'@ | image-slim-agent.exe --allow-root D:\Pictures compress --request -
```

成功和失败分别使用以下信封：

```json
{"ok":true,"result":{"state":"completed"}}
{"ok":false,"error":{"code":"root_not_allowed","params":{},"path":null,"detail":null,"retryable":false}}
```

## MCP 工具

| 工具 | 行为 |
| --- | --- |
| `image_slim_capabilities` | 返回协议版本、格式、限制和进程权限 |
| `image_slim_plan` | 扫描路径并建立 30 分钟内可复用的内存计划 |
| `image_slim_compress` | 使用 `plan_id` 或路径启动压缩；最多等待 `wait_ms` |
| `image_slim_status` | 返回任务汇总和最多 50 个分页问题 |
| `image_slim_cancel` | 幂等地请求协作取消 |

Agent 同时只运行一个批次。计划最多保留 4 个，完成任务最多保留 32 个，幂等请求结果最多保留
128 个；进程退出后全部消失。协议版本为 `agent_protocol_version: 1`。

## Codex 配置示例

Codex 的用户级 `config.toml` 支持为 stdio MCP server 设置 `command` 与 `args`。以下内容需要用户
按实际安装路径手工加入，image-slim 不会自动修改 Codex 配置：

```toml
[mcp_servers.image_slim]
command = 'C:\Program Files\image-slim\image-slim-agent.exe'
args = ["--allow-root", 'D:\Pictures', "mcp"]
tool_timeout_sec = 3600
```

字段含义见 [Codex 配置参考](https://developers.openai.com/codex/config-reference/)。

## Claude Desktop 配置示例

在 Claude Desktop 的 `mcpServers` 中手工加入：

```json
{
  "mcpServers": {
    "image-slim": {
      "command": "C:\\Program Files\\image-slim\\image-slim-agent.exe",
      "args": ["--allow-root", "D:\\Pictures", "mcp"]
    }
  }
}
```

需要多个根目录时重复 `--allow-root` 和路径；只有确需覆盖原图时才加入 `--allow-overwrite`。
