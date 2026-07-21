import {
  copyFileSync,
  createReadStream,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync
} from 'node:fs';
import { createHash } from 'node:crypto';
import { stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = fileURLToPath(new URL('../', import.meta.url));
const readJson = (relativePath) => JSON.parse(
  readFileSync(path.join(projectRoot, relativePath), 'utf8')
);
const packageJson = readJson('package.json');
const tauriConfig = readJson('src-tauri/tauri.conf.json');
const agentTauriConfig = readJson('src-tauri/tauri.agent.conf.json');
const args = new Set(process.argv.slice(2));
const allowedArgs = new Set(['--check', '--no-bundle']);

for (const argument of args) {
  if (!allowedArgs.has(argument)) fail(`Unknown argument: ${argument}`);
}

const checkOnly = args.has('--check');
const noBundle = args.has('--no-bundle');
const productName = tauriConfig.productName;
const version = packageJson.version;
const releaseDirectory = path.join(projectRoot, 'release');
const targetDirectory = path.join(projectRoot, 'src-tauri', 'target', 'release');
const installerName = `${productName}_${version}_x64-setup.exe`;
const portableName = `${productName}_${version}_x64-portable.exe`;
const agentName = `${productName}-agent_${version}_x64.exe`;
const licenseNames = ['LICENSE', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_LICENSES.txt'];
const maxSizes = new Map([
  [portableName, 15 * 1024 * 1024],
  [agentName, 15 * 1024 * 1024],
  [installerName, 10 * 1024 * 1024]
]);

if (typeof productName !== 'string' || !productName) fail('Tauri productName is missing');
if (typeof version !== 'string' || !version) fail('package.json version is missing');
if (tauriConfig.version !== '../package.json') {
  fail('Tauri must read its version from ../package.json');
}
if (!tauriConfig.bundle?.targets?.includes('nsis')) {
  fail('Tauri bundle targets must include NSIS');
}
if (!agentTauriConfig.bundle?.externalBin?.includes('binaries/image-slim-agent')) {
  fail('Tauri bundle must include the image-slim Agent sidecar');
}
for (const name of licenseNames) requireFile(path.join(projectRoot, name));

if (checkOnly) {
  console.log(`Release staging is configured for ${portableName}, ${agentName}, and ${installerName}.`);
  process.exit(0);
}

const files = [
  {
    source: path.join(targetDirectory, `${productName}.exe`),
    destinationName: portableName
  },
  {
    source: path.join(targetDirectory, `${productName}-agent.exe`),
    destinationName: agentName
  },
  ...(!noBundle ? [{
    source: path.join(targetDirectory, 'bundle', 'nsis', installerName),
    destinationName: installerName
  }] : []),
  ...licenseNames.map((name) => ({
    source: path.join(projectRoot, name),
    destinationName: name
  }))
];

files.forEach(({ source }) => requireFile(source));
mkdirSync(releaseDirectory, { recursive: true });

const staged = [];
for (const file of files) {
  const destination = path.join(releaseDirectory, file.destinationName);
  const sourceHash = await sha256(file.source);
  const temporary = `${destination}.tmp-${process.pid}-${Date.now()}`;
  try {
    copyFileSync(file.source, temporary);
    const temporaryHash = await sha256(temporary);
    if (temporaryHash !== sourceHash) {
      fail(`Hash mismatch while staging ${file.destinationName}`);
    }
    renameSync(temporary, destination);
  } finally {
    rmSync(temporary, { force: true });
  }
  const size = (await stat(destination)).size;
  const maxSize = maxSizes.get(file.destinationName);
  if (maxSize && size > maxSize) {
    fail(`${file.destinationName} is ${size} bytes; limit is ${maxSize} bytes`);
  }
  staged.push({ name: file.destinationName, path: destination, hash: sourceHash, size });
}

const checksumPath = path.join(releaseDirectory, 'SHA256SUMS.txt');
const checksumTemporary = `${checksumPath}.tmp-${process.pid}-${Date.now()}`;
const checksumText = staged.map((file) => `${file.hash}  ${file.name}`).join('\n') + '\n';
try {
  writeFileSync(checksumTemporary, checksumText, 'utf8');
  renameSync(checksumTemporary, checksumPath);
} finally {
  rmSync(checksumTemporary, { force: true });
}

for (const file of staged) {
  console.log(`${file.path}\t${file.size} bytes\tSHA-256 ${file.hash}`);
}
console.log(`${checksumPath}\t${Buffer.byteLength(checksumText, 'utf8')} bytes`);

function requireFile(filePath) {
  if (!existsSync(filePath)) fail(`Required release file is missing: ${filePath}`);
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

function fail(message) {
  console.error(message);
  process.exit(1);
}
