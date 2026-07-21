import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import {
  copyFileSync,
  createReadStream,
  mkdirSync,
  renameSync,
  rmSync
} from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = fileURLToPath(new URL('../', import.meta.url));
const triple = execFileSync('rustc', ['--print', 'host-tuple'], { encoding: 'utf8' }).trim();
if (triple !== 'x86_64-pc-windows-msvc') {
  throw new Error(`Agent packaging requires x86_64-pc-windows-msvc, received ${triple}`);
}

const source = path.join(projectRoot, 'src-tauri', 'target', 'release', 'image-slim-agent.exe');
const directory = path.join(projectRoot, 'src-tauri', 'binaries');
const destination = path.join(directory, `image-slim-agent-${triple}.exe`);
const temporary = `${destination}.tmp-${process.pid}-${Date.now()}`;
mkdirSync(directory, { recursive: true });

try {
  copyFileSync(source, temporary);
  const [sourceHash, copiedHash] = await Promise.all([sha256(source), sha256(temporary)]);
  if (sourceHash !== copiedHash) throw new Error('Agent sidecar hash mismatch after copying');
  renameSync(temporary, destination);
  console.log(`${destination}\tSHA-256 ${sourceHash}`);
} finally {
  rmSync(temporary, { force: true });
}

function sha256(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('error', reject);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('end', () => resolve(hash.digest('hex')));
  });
}
