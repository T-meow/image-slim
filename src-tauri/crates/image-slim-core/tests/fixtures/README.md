# Audio fixtures

Synthetic two-second tones at 44.1 kHz, generated for this project with the
adjacent `generate.mjs` script and FFmpeg 8.1.1. They contain no recordings,
personal information, or third-party media. The samples and generator use the
project's GPL-3.0-or-later license.

Run `node src-tauri/crates/image-slim-core/tests/fixtures/generate.mjs` from the
project root to recreate them. FFmpeg is needed only to recreate fixtures;
tests and the shipped application use the bundled Rust/C audio codecs.
