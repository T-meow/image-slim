# image-slim 全面优化实施计划

## 目标

在保持完全离线、Windows x64、静态 PNG/JPEG/WebP 与既有原子输出安全语义的前提下，
解决大图内存风险、扫描与预览取消不完整、混合输入映射不稳定、错误不可理解、大队列更新缓慢、
IPC 类型易漂移及公开发布保护线不足等问题。

## 范围与非目标

- 输入上限固定为单文件 512 MiB、单图 100,000,000 像素、任一边 65,535 像素、队列 10,000 项。
- 目标平台为 Windows 10/11 x64，最低按 8GB 内存设计；内存不足时拒绝任务而不是冒险分配。
- 本轮不增加自定义输出根目录、多选处理、筛选排序、格式转换、历史记录或跨平台支持。
- 暂停 GitHub Pages；不执行 Git commit、push、Release 或远端设置修改。

## 实施阶段

1. 删除 Pages 部署入口，建立 Windows CI，并分离生产/开发 CSP。
2. 加入输入限制、结构化错误、自适应内存调度、流式内容校验和安全写入 guard。
3. 将扫描改为可取消的分批事件流，稳定重叠路径映射，并在界面展示问题详情。
4. 将预览改成 single-flight，限制显示图尺寸，缓存并安全复用候选结果，减少重复解码。
5. 使用 Map 队列、增量计数和虚拟列表，生成 IPC TypeScript 类型，并统一能力与版本来源。
6. 补齐边界、取消、缓存、队列、组件、CSP、版本与构建测试，完成 release 验证和基线记录。

## 兼容与数据风险

- 保留现有 localStorage 键；元数据策略值从 `all` 自动迁移为 `supported`。
- 不持久化队列或预览缓存；预览缓存启动时清理，候选仅在源内容和候选内容哈希均匹配时复用。
- 输入超限、队列满、内存不足均作为结构化问题返回，不影响同次扫描中的其他有效文件。
- 覆盖模式继续执行源文件变化检查、路径边界检查、同目录临时文件写入和原子替换。

## 验证计划

- 前端：类型检查、工具函数与 store 测试、错误本地化、设置迁移、问题面板和虚拟列表测试。
- Rust：输入边界、扫描顺序、调度预算、取消、BLAKE3 内容变化、缓存、编解码和原子写入测试。
- 配置：IPC bindings、版本一致性、生产 CSP、Pages 停用和 Windows CI 配置检查。
- 构建：README 中全部检查、Tauri debug 启动烟测、根目录 `release/` 归档及产物体积检查。

## 假设

- 100MP 是格式上限，不保证在任何时刻均可执行；可用内存不足时给出明确提示。
- 原生编码器没有安全中断点时采用协作取消，当前阶段完成后停止，不强制终止线程。
- 性能优化优先保证输出安全和可诊断性，不以降低校验强度换取吞吐。

## 实施结果（2026-07-21）

- 六个阶段均已实施；GitHub Pages 工作流及 README 部署说明已移除。若仓库远端此前已启用 Pages，
  仍需维护者在 GitHub Settings 中人工关闭，本次未执行远端设置修改。
- 前端 `svelte-check` 零错误零警告，Vitest 7 个文件共 16 项测试通过，生产构建通过。
- Rust 35 项常规测试通过，1 项 release 性能基线测试按设计默认忽略；`fmt`、`clippy -D warnings`、
  `test` 与 `check --all-targets` 通过。
- IPC bindings、npm/Cargo 版本、Pages/CSP/Node/Rust CI 配置检查通过；Tauri debug 应用成功启动并清理。
- 12MP/48MP、三格式、三档共 18 组 release 数据已记录于
  [`performance-baseline.zh-CN.md`](performance-baseline.zh-CN.md)。
- 上次基线 EXE：`src-tauri/target/release/image-slim.exe`，9,921,024 字节（9.461 MiB），
  SHA-256 `7E0DED8762EE902B099246B82F4755C0DB932AFFCF289D493855A6C0463FF2C1`。
- 上次基线 NSIS：`src-tauri/target/release/bundle/nsis/image-slim_0.1.0_x64-setup.exe`，
  2,734,816 字节（2.608 MiB），
  SHA-256 `26FB26AF01BEFB8E193CC8E083F49563B5D7432C5339855DA545DA43C3EE7E61`。

## 队列修复与统一发布目录（2026-07-21）

- 修复 `QueueController` 仅发布版本数字、界面读取非响应式 `ids/count` 导致扫描结果不显示的问题；
  现在发布包含稳定 ID 顺序、数量、版本和增量汇总的不可变 `QueueSnapshot`。
- 扫描完成前不再选择项目或启动压缩预览；完成后等待 400ms 空闲时间再生成预览。任务缩略图使用
  `loading="lazy"` 与 `decoding="async"`，减少添加大批图片时的主线程和解码压力。
- 扫描与批处理事件监听全部注册成功后才开放添加入口；监听失败会保留结构化错误并保持入口禁用。
- 新增 jsdom 客户端流程测试，覆盖添加、分批事件、队列显示、开始按钮和清空；前端共 8 个测试文件、
  19 项测试通过，`svelte-check` 零错误零警告。Rust 35 项常规测试通过，1 项性能基线测试按设计忽略。
- 本次修改前源码快照：`D:\Projects\_backups\image-slim-20260721-122159`。原始恢复版 EXE 的
  SHA-256 仍为 `FC18A58F76D53E828CE6B2AD9196EA591E445A9C98E440909101225AA7D2068A`，未用于替换。
- 最终便携版：`release/image-slim_0.1.0_x64-portable.exe`，9,919,488 字节，
  SHA-256 `7C981F17080323989AED17C7B00D056FC5F40DC44CD4F1A123F9F3572AD96552`。
- 最终 NSIS：`release/image-slim_0.1.0_x64-setup.exe`，2,734,939 字节，
  SHA-256 `99A48A972AC8FAA799F3A8D70BA0BDABE5C5FFB52BB85D3130961607EA36A065`。
- `npm run tauri:build` 与 `npm run tauri:build:no-bundle` 会自动生成许可文件并将最终文件原子归档到
  根目录 `release/`；`SHA256SUMS.txt` 已在本机复核，便携版独立启动烟测通过。
