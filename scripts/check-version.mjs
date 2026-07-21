import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const packageVersion = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
const metadata = JSON.parse(execFileSync('cargo', [
  'metadata',
  '--no-deps',
  '--format-version',
  '1',
  '--manifest-path',
  fileURLToPath(new URL('../src-tauri/Cargo.toml', import.meta.url))
], { encoding: 'utf8' }));
const cargoVersion = metadata.packages.find((entry) => entry.name === 'image-slim')?.version;

if (!cargoVersion || cargoVersion !== packageVersion) {
  console.error(`Version mismatch: package.json=${packageVersion}, Cargo.toml=${cargoVersion ?? 'missing'}`);
  process.exit(1);
}

console.log(`Version ${packageVersion} is consistent.`);
