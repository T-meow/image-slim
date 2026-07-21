import { existsSync, readFileSync } from 'node:fs';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const tauri = JSON.parse(read('src-tauri/tauri.conf.json'));
const csp = tauri.app?.security?.csp ?? '';
const devCsp = tauri.app?.security?.devCsp ?? '';
const assetScope = tauri.app?.security?.assetProtocol?.scope ?? [];
const ci = read('.github/workflows/ci.yml');
const vite = read('vite.config.ts');
const gitignore = read('.gitignore');
const packageJson = JSON.parse(read('package.json'));
const failures = [];

if (existsSync(new URL('../.github/workflows/pages.yml', import.meta.url))) {
  failures.push('GitHub Pages workflow must remain disabled');
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
if (!packageJson.scripts?.['tauri:build:no-bundle']?.includes('scripts/stage-release.mjs --no-bundle')) {
  failures.push('The no-bundle Tauri build does not stage files into release/');
}
if (!/^release\/$/m.test(gitignore)) {
  failures.push('The local release artifact directory must be ignored by Git');
}
if (!existsSync(new URL('../scripts/stage-release.mjs', import.meta.url))) {
  failures.push('The release staging script is missing');
}
if (!/base:\s*['"]\.\/['"]/.test(vite)) {
  failures.push('Vite must retain a relative production asset base');
}

if (failures.length) {
  failures.forEach((failure) => console.error(failure));
  process.exit(1);
}

console.log('Pages, CSP, and Tauri version configuration are consistent.');
