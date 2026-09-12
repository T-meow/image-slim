// Developer-only fixture generation. Runtime compression does not use FFmpeg.
import { spawnSync } from 'node:child_process';
import { statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const directory = fileURLToPath(new URL('.', import.meta.url));
const signal = 'aevalsrc=0.2*sin(2*PI*997*t)+0.1*sin(2*PI*211*t)+0.05*sin(2*PI*7001*t):s=44100:d=2';
const cases = [
  ['tone.wav', '2', ['-c:a', 'pcm_s16le']],
  ['mono.wav', '1', ['-c:a', 'pcm_s16le']],
  ['tone.mp3', '2', ['-c:a', 'libmp3lame', '-b:a', '320k']],
  ['tone.flac', '2', ['-c:a', 'flac']],
  ['tone.m4a', '2', ['-c:a', 'aac', '-b:a', '256k']],
  ['alac.m4a', '2', ['-c:a', 'alac']],
  ['tone.ogg', '2', ['-c:a', 'libvorbis', '-q:a', '8']]
];
for (const [name, channels, codec] of cases) {
  const destination = join(directory, name);
  const result = spawnSync('ffmpeg', [
    '-hide_banner', '-loglevel', 'error', '-nostdin', '-y', '-f', 'lavfi', '-i', signal,
    '-ac', channels, ...codec, '-threads', '1', '-map_metadata', '-1', destination
  ], { encoding: 'utf8', windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr);
  console.log(`${name}: ${statSync(destination).size} bytes`);
}
