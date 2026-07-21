import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const packageVersion = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
const releaseVersion = `v${packageVersion}`;
const metadata = JSON.parse(execFileSync('cargo', [
  'metadata',
  '--no-deps',
  '--format-version',
  '1',
  '--manifest-path',
  fileURLToPath(new URL('../src-tauri/Cargo.toml', import.meta.url))
], { encoding: 'utf8' }));
const expectedPackages = ['image-slim', 'image-slim-agent', 'image-slim-core'];
for (const name of expectedPackages) {
  const cargoVersion = metadata.packages.find((entry) => entry.name === name)?.version;
  if (!cargoVersion || cargoVersion !== packageVersion) {
    console.error(`Version mismatch: package.json=${packageVersion}, ${name}=${cargoVersion ?? 'missing'}`);
    process.exit(1);
  }
}

const versionedFiles = [
  `image-slim_${packageVersion}_x64-setup.exe`,
  `image-slim_${packageVersion}_x64-portable.exe`,
  `image-slim-agent_${packageVersion}_x64.exe`
];
for (const path of ['../README.md', '../README.en.md', '../site/index.html']) {
  const content = readFileSync(new URL(path, import.meta.url), 'utf8');
  if (!content.includes(`/releases/tag/${releaseVersion}`)) {
    console.error(`${path} does not link to release ${releaseVersion}`);
    process.exit(1);
  }
  for (const file of versionedFiles) {
    if (!content.includes(`/releases/download/${releaseVersion}/${file}`)) {
      console.error(`${path} does not link to ${file} in release ${releaseVersion}`);
      process.exit(1);
    }
  }
}

console.log(`Version ${packageVersion} is consistent across GUI, Agent, core, README, and Pages.`);
