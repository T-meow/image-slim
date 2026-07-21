import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const cargoRoot = join(root, 'src-tauri');
const licensePattern = /^(license|copying|copyright|notice)([._-].*)?$/i;

function licenseFiles(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && licensePattern.test(entry.name))
    .map((entry) => join(directory, entry.name))
    .sort();
}

function readableText(path) {
  const content = readFileSync(path);
  if (content.includes(0)) return '';
  return content.toString('utf8').trim();
}

function section(title, license, homepage, files) {
  const lines = [`\n${'='.repeat(78)}`, title, `License: ${license || 'not declared'}`];
  if (homepage) lines.push(`Source: ${homepage}`);
  for (const file of files) {
    const content = readableText(file);
    if (content) lines.push(`\n--- ${file.split(/[\\/]/).at(-1)} ---\n${content}`);
  }
  return lines.join('\n');
}

function cargoSections() {
  const metadata = JSON.parse(execFileSync('cargo', [
    'metadata',
    '--format-version', '1',
    '--locked',
    '--filter-platform', 'x86_64-pc-windows-msvc'
  ], { cwd: cargoRoot, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 }));
  const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const included = new Set();
  const visited = new Set();
  const queue = [...metadata.workspace_members];
  while (queue.length) {
    const id = queue.shift();
    if (visited.has(id)) continue;
    visited.add(id);
    const node = nodes.get(id);
    if (!node) continue;
    for (const dependency of node.deps) {
      if (!dependency.dep_kinds.some((kind) => kind.kind === null)) continue;
      if (!included.has(dependency.pkg)) {
        included.add(dependency.pkg);
        queue.push(dependency.pkg);
      }
    }
  }
  return [...included]
    .map((id) => packages.get(id))
    .filter((pkg) => pkg?.source)
    .sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`))
    .map((pkg) => section(
      `Rust: ${pkg.name} ${pkg.version}`,
      pkg.license,
      pkg.repository || pkg.homepage,
      licenseFiles(dirname(pkg.manifest_path))
    ));
}

function nodeSections() {
  const lock = JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8'));
  const seen = new Set();
  const packages = [];
  for (const [packagePath, locked] of Object.entries(lock.packages)) {
    if (!packagePath || locked.dev === true || !packagePath.startsWith('node_modules/')) continue;
    const directory = join(root, packagePath);
    const manifestPath = join(directory, 'package.json');
    if (!existsSync(manifestPath) || !statSync(manifestPath).isFile()) continue;
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    const key = `${manifest.name}@${manifest.version}`;
    if (seen.has(key)) continue;
    seen.add(key);
    packages.push({ key, directory, manifest });
  }
  return packages
    .sort((left, right) => left.key.localeCompare(right.key))
    .map(({ key, directory, manifest }) => section(
      `Frontend: ${key}`,
      manifest.license,
      typeof manifest.repository === 'string' ? manifest.repository : manifest.repository?.url,
      licenseFiles(directory)
    ));
}

function adaptedSourceSection() {
  const directory = join(root, 'third-party', 'losslessly-0.1.1');
  const license = join(directory, 'LICENSE');
  if (!existsSync(license)) {
    throw new Error('The bundled license for adapted losslessly 0.1.1 source was not found');
  }
  return [section(
    'Adapted source: losslessly 0.1.1',
    'MIT',
    'https://github.com/kdoroszewicz/losslessly',
    [license]
  )];
}

const introduction = `image-slim 0.1.0 third-party licenses

Generated from package-lock.json and src-tauri/Cargo.lock for the Windows x64
release build. Build-only and development-only packages are excluded where the
package managers expose that distinction. The losslessly attribution is added
explicitly because source was adapted rather than linked as a dependency.`;
const output = [introduction, ...adaptedSourceSection(), ...cargoSections(), ...nodeSections()]
  .join('\n')
  .concat('\n');
writeFileSync(join(root, 'THIRD_PARTY_LICENSES.txt'), output, 'utf8');
console.log(`Wrote THIRD_PARTY_LICENSES.txt (${output.length} characters)`);
