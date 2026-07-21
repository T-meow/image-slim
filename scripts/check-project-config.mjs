import { existsSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const tauri = JSON.parse(read('src-tauri/tauri.conf.json'));
const agentTauri = JSON.parse(read('src-tauri/tauri.agent.conf.json'));
const csp = tauri.app?.security?.csp ?? '';
const devCsp = tauri.app?.security?.devCsp ?? '';
const assetScope = tauri.app?.security?.assetProtocol?.scope ?? [];
const ci = read('.github/workflows/ci.yml');
const pages = read('.github/workflows/pages.yml');
const pagesIndex = read('site/index.html');
const vite = read('vite.config.ts');
const gitignore = read('.gitignore');
const packageJson = JSON.parse(read('package.json'));
const failures = [];

if (!/pages:\s*write/.test(pages) || !/id-token:\s*write/.test(pages)) {
  failures.push('GitHub Pages workflow is missing deployment permissions');
}
for (const action of [
  'actions/configure-pages@v6',
  'actions/upload-pages-artifact@v5',
  'actions/deploy-pages@v5'
]) {
  if (!pages.includes(action)) {
    failures.push(`GitHub Pages workflow is missing ${action}`);
  }
}
if (!/path:\s*site\b/.test(pages)) {
  failures.push('GitHub Pages workflow must publish only the site/ directory');
}
if (!pagesIndex.includes('https://github.com/T-meow/image-slim/releases/download/')) {
  failures.push('GitHub Pages download links are missing');
}
if (/127\.0\.0\.1|localhost:\d|ws:\/\//i.test(csp)) {
  failures.push('Production CSP contains a development HTTP/WebSocket endpoint');
}
if (!devCsp.includes('http://127.0.0.1:1421') || !devCsp.includes('ws://127.0.0.1:1421')) {
  failures.push('Development CSP is missing the Vite HTTP/WebSocket endpoints');
}
for (const pattern of [
  '**/*.[pP][nN][gG]',
  '**/*.[jJ][pP][gG]',
  '**/*.[jJ][pP][eE][gG]',
  '**/*.[wW][eE][bB][pP]'
]) {
  if (!assetScope.includes(pattern)) {
    failures.push(`Asset protocol scope does not cover mixed-case image extensions: ${pattern}`);
  }
}
if (tauri.version !== '../package.json') {
  failures.push('Tauri version must be read from ../package.json');
}
if (!/node-version:\s*24\b/.test(ci) || !/toolchain:\s*1\.96\.0\b/.test(ci)) {
  failures.push('Windows CI must pin Node.js 24 and Rust 1.96.0');
}
if (!ci.includes('npm run tauri:build:no-bundle')) {
  failures.push('Main-branch CI is missing the Tauri --no-bundle build');
}
if (!ci.includes('npm run release:check')) {
  failures.push('CI is missing the release staging configuration check');
}
if (!packageJson.scripts?.['tauri:build']?.includes('scripts/stage-release.mjs')) {
  failures.push('The bundled Tauri build does not stage files into release/');
}
if (!packageJson.scripts?.['agent:build']?.includes('-p image-slim-agent --release --locked')) {
  failures.push('Agent release build script is missing or is not locked');
}
if (!packageJson.scripts?.['agent:build']?.includes('prepare-agent-sidecar.mjs')) {
  failures.push('Agent sidecar preparation is missing from the release build');
}
if (!agentTauri.bundle?.externalBin?.includes('binaries/image-slim-agent')) {
  failures.push('Tauri must bundle image-slim-agent as an external binary');
}
if (!packageJson.scripts?.['tauri:build:no-bundle']?.includes('scripts/stage-release.mjs --no-bundle')) {
  failures.push('The no-bundle Tauri build does not stage files into release/');
}
if (!/^release\/$/m.test(gitignore)) {
  failures.push('The local release artifact directory must be ignored by Git');
}
if (!existsSync(new URL('../scripts/stage-release.mjs', import.meta.url))) {
  failures.push('The release staging script is missing');
}
if (!existsSync(new URL('../scripts/prepare-agent-sidecar.mjs', import.meta.url))) {
  failures.push('The Agent sidecar preparation script is missing');
}
if (!existsSync(new URL('../third-party/losslessly-0.1.1/LICENSE', import.meta.url))) {
  failures.push('The bundled license for adapted losslessly 0.1.1 source is missing');
}
if (!ci.includes('cargo test --workspace --locked')) {
  failures.push('Windows CI must test every Cargo workspace package');
}
if (!ci.includes('image-slim-agent.exe')) {
  failures.push('Windows CI is missing the Agent executable smoke check');
}
if (!/base:\s*['"]\.\/['"]/.test(vite)) {
  failures.push('Vite must retain a relative production asset base');
}

for (const packageName of ['image-slim-core', 'image-slim-agent']) {
  const tree = execFileSync('cargo', [
    'tree', '-p', packageName, '--edges', 'normal', '--prefix', 'none', '--locked'
  ], { cwd: new URL('../src-tauri/', import.meta.url), encoding: 'utf8' });
  if (/^tauri v/m.test(tree)) {
    failures.push(`${packageName} must not depend on Tauri`);
  }
}

if (failures.length) {
  failures.forEach((failure) => console.error(failure));
  process.exit(1);
}

console.log('Pages, CSP, and Tauri version configuration are consistent.');
