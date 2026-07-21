<p align="center">
  <img src="assets/icon.svg" width="88" alt="image-slim icon">
</p>

<h1 align="center">image-slim</h1>

<p align="center">A fully offline batch image compressor for Windows.</p>

<p align="center">
  <a href="README.md">简体中文</a> · <strong>English</strong>
</p>

<p align="center">
  <a href="https://t-meow.github.io/image-slim/"><strong>Website & downloads</strong></a> ·
  <a href="https://github.com/T-meow/image-slim/releases/tag/v0.1.0">Release v0.1.0</a> ·
  <a href="https://github.com/T-meow/image-slim/issues">Issues</a>
</p>

image-slim is built with Tauri 2, Svelte 5, and Rust. It compresses PNG, JPEG,
and WebP files locally without uploads, accounts, or telemetry. The current version
is `0.1.0` and supports Windows 10/11 x64 only.

## Download

| Edition | Use case | Download |
|---|---|---|
| Windows installer | Recommended; installs the GUI and Agent | [`image-slim_0.1.0_x64-setup.exe`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/image-slim_0.1.0_x64-setup.exe) |
| Portable GUI | Single-file desktop app, no install required | [`image-slim_0.1.0_x64-portable.exe`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/image-slim_0.1.0_x64-portable.exe) |
| Standalone Agent | JSON CLI / MCP stdio automation | [`image-slim-agent_0.1.0_x64.exe`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/image-slim-agent_0.1.0_x64.exe) |

See [`SHA256SUMS.txt`](https://github.com/T-meow/image-slim/releases/download/v0.1.0/SHA256SUMS.txt)
for complete checksums. Version `0.1.0` is not code-signed, so Windows SmartScreen may show an
unknown-publisher warning. Verify a download with `Get-FileHash <path> -Algorithm SHA256`.

## Features

- Drop multiple files, folders, or mixed inputs; folders are scanned recursively while preserving relative paths.
- Choose from Lossless, Balanced, and Strong presets; the original is kept when a candidate is not smaller.
- Process batches with retries, cancellation, output statistics, and a before/after comparison slider.
- Write to an editable `compressed` subfolder or replace originals after one batch-level confirmation.
- Detect external source changes before replacement and atomically replace from a same-directory temporary file.
- Remove privacy-sensitive metadata by default while preserving display-critical information, or retain supported metadata.
- Switch between Simplified Chinese/English and System/Light/Dark themes; preferences are stored locally.
- Use the installed `image-slim-agent.exe` through JSON CLI or MCP stdio within explicit allowed roots.

## GUI, CLI, and MCP

The desktop GUI handles everyday batches with drag and drop, queues, previews, retries, and output-location
actions. The Agent uses the same `image-slim-core` for scripts and AI tool automation. It does not launch a
GUI, listen on a network port, or return image bytes.

```powershell
# Inspect capabilities; no path permission is required
image-slim-agent.exe capabilities --json

# Read a plan request from stdin; CLI plan does not create a cross-process plan_id
'{"request_id":"11111111-1111-4111-8111-111111111111","paths":["D:\\Pictures"]}' |
  image-slim-agent.exe --allow-root D:\Pictures plan --request -

# MCP stdio; status and cancel are only available in this persistent process
image-slim-agent.exe --allow-root D:\Pictures mcp
```

MCP exposes `image_slim_capabilities`, `image_slim_plan`, `image_slim_compress`,
`image_slim_status`, and `image_slim_cancel`. Reads must stay inside explicit `--allow-root`
boundaries. Replacing originals additionally requires both process-level `--allow-overwrite` and request-level
authorization. See [`docs/agent-protocol.zh-CN.md`](docs/agent-protocol.zh-CN.md) for the full protocol and
Codex/Claude configuration examples.

## Supported Files

| Format | Supported | Explicitly unsupported |
|---|---|---|
| PNG | Static 8/16-bit PNG, indexed color, alpha | APNG |
| JPEG | RGB/grayscale, baseline/progressive JPEG | CMYK/YCCK JPEG |
| WebP | Static VP8/VP8L WebP, alpha | Animated WebP |

Files with mismatched extensions and signatures, corrupt files, and symbolic links are
reported individually and skipped. Limits are inclusive; an over-limit item does not stop
other queued files:

- Maximum file size: `512 MiB` (`536,870,912` bytes).
- Maximum image size: `100,000,000` pixels and `65,535` pixels on either dimension.
- Maximum queue size: `10,000` images; remaining directories are not traversed after the limit.
- Peak memory is estimated per format while reserving memory for Windows and the WebView. A
  format-valid image is still rejected clearly when current available memory is insufficient.

The first release does not include AVIF, format conversion, resizing, GIF, target-size
compression, or editing.

## Presets

| Preset | PNG | JPEG | WebP |
|---|---|---|---|
| Lossless | OxiPNG optimization with full decoded-pixel verification | Preserves DCT coefficients; optimizes coding and progressive scans only | Cleans the container; also tries exact lossless re-encoding for VP8L |
| Balanced | Medium-quality libimagequant quantization followed by OxiPNG | MozJPEG quality 82, 4:2:0, progressive, and trellis | libwebp quality 80, method 6, sharp YUV |
| Strong | Lower color budget and slower quantization followed by OxiPNG | MozJPEG quality 68 with the same optimizations | libwebp quality 65, method 6, and lower alpha quality |

Every candidate is decoded again to verify its format, dimensions, and integrity. Lossless
results are also compared at the decoded-pixel level. PNG verification includes hidden RGB
values under fully transparent pixels. A candidate is only used when it is smaller.

## Output Safety

Subfolder mode writes to `compressed` under each input root by default and preserves relative
paths. Replace mode does not create `.bak` files, so it requires one confirmation before a batch starts.

Replacement is protected by the following sequence:

1. Input paths are normalized and deduplicated; symbolic links and calculated output roots are excluded.
2. The result is written to a unique same-directory temporary file and synchronized to disk.
3. The source path, size, modification time, and complete byte content are checked again.
4. Cancellation and output-boundary checks run immediately before a Windows atomic replacement.
5. If the candidate is not smaller, replace mode leaves the source untouched and subfolder mode copies it.

## Usage

1. Select images or folders from the toolbar, or drop them onto the window.
2. Choose a compression preset, output mode, and metadata policy.
3. Select an item to inspect its preview, synchronized zoom, and comparison slider.
4. Select **Compress**. Existing outputs and source replacement require confirmation first.
5. Open completed output locations from the queue, or retry failed items individually.

## Run From Source

### Requirements

- Windows 10/11 x64 and Microsoft Edge WebView2 Runtime.
- Node.js 24 and npm 11 (currently tested versions).
- Rust `1.93+`; release builds are currently tested with Rust `1.96`.
- Visual Studio 2022 Build Tools with the MSVC C++ toolchain and Windows SDK.

```powershell
npm ci
npm run tauri:dev
```

For networks where the npm mirror is useful:

```powershell
npm ci --registry=https://registry.npmmirror.com/
```

## Validation

```powershell
npm run check
npm test
npm run build
npm run ipc:check
npm run version:check
npm run config:check

Push-Location src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo check --workspace --all-targets --locked
Pop-Location
```

The current tests exercise real PNG/JPEG/WebP codecs, 16-bit PNG, transparent pixels,
input boundaries, chunked scanning, deduplication and capacity, memory scheduling, preview
caching and cancellation, the virtual queue, settings migration, metadata cleanup, no-gain
fallback, BLAKE3 source-change detection, and guarded atomic writes.

## Build Release Artifacts

Build the Windows x64 NSIS installer, GUI executable, and Agent executable, including the license collection
and checksums:

```powershell
npm run tauri:build
```

Final artifacts are staged in the project-root `release/` directory with versioned names,
license files, and `SHA256SUMS.txt`. To build both standalone executables without the installer:

```powershell
npm run tauri:build:no-bundle
```

`src-tauri/target/` remains a build cache and intermediate-output directory. Version `0.1.0`
is not code-signed, so Windows SmartScreen may show an unknown-publisher warning.

The static download site lives in `site/` and is deployed from `main` by the
[Pages workflow](.github/workflows/pages.yml) to <https://t-meow.github.io/image-slim/>.

## Project Layout

```text
src/                    Svelte UI, state, and Tauri IPC wrapper
src-tauri/src/          Tauri commands, GUI state, and event adapter
src-tauri/crates/       Shared core plus Agent/CLI/MCP crates
src-tauri/capabilities/ Tauri capability boundaries
scripts/                Configuration, version, and third-party license checks
docs/                   Chinese implementation and build notes
release/                Local versioned release artifacts (ignored by Git)
```

See [`docs/codec-build.zh-CN.md`](docs/codec-build.zh-CN.md) for locked codec versions,
static-linking details, and reproducible build notes.
The 12MP/48MP release performance measurements are recorded in
[`docs/performance-baseline.zh-CN.md`](docs/performance-baseline.zh-CN.md).
Agent permissions, JSON CLI, MCP tools, and host examples are documented in
[`docs/agent-protocol.zh-CN.md`](docs/agent-protocol.zh-CN.md).

## Privacy and Networking

The application makes no runtime network requests and contains no cloud sync, accounts,
telemetry, or automatic updater. Image data, paths, preview caches, and output files remain
on the local machine. Preview caches are removed the next time the application starts.

## Contributing

Issues and pull requests are welcome. Before submitting code, run every command in
**Validation** and preserve the existing Windows x64, static PNG/JPEG/WebP, and output-safety
contracts. Changes that add formats, large dependencies, or network behavior should explain
their size, licensing, privacy, and failure-handling impact first.

## License

image-slim is released under [GPL-3.0-or-later](LICENSE), in part because the libimagequant
PNG quantizer is licensed under GPL-3.0-or-later. See
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) and
[`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt) for attribution and full license texts.
