// Compile the current Managed command/acquisition/output path against native
// Arduino fakes. setup() and the platform headers remain in the AVR build.
import {readFileSync, writeFileSync} from 'node:fs';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = readFileSync(resolve(repo, 'firmware/loadcell_7ch/src/main.cpp'), 'utf8');
const start = source.indexOf('namespace {');
const end = source.indexOf('\n}  // namespace', start);
const loop = source.indexOf('\nvoid loop() {', end);
if (start < 0 || end < start || loop < end) throw Error('Managed firmware sections moved');
if (process.argv[2] !== '--emit-cpp' || !process.argv[3]) {
  throw Error('Usage: node tools/test-firmware-quality-stream.mjs --emit-cpp ABSOLUTE_OUTPUT.inc');
}
// Preserve the complete namespace and the actual loop, including the current
// queue, fresh-flag handling, session transitions and command dispatch.
writeFileSync(resolve(process.argv[3]), source.slice(start, end + '\n}  // namespace'.length)
  + source.slice(loop));
