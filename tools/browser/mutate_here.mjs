// CrystalOSD References/scripts/mutate.mjs for verifiers that live outside the lab's References/scripts: the same mutants (numeric
// literals nudged, + - * < <= > >= swapped, on lines that compute) made in `--target` (the verifier itself, or a model it imports), each
// run in a copy of this folder whose relative imports of the lab and of this folder are made absolute, so mutants never touch the
// real files and can run in parallel. A mutant is killed when the verifier's last verdict is not FOUND.
//   node tools/browser/mutate_here.mjs <verify_x.mjs> [--target <file.mjs>] --capture <name> [--env K=V,...] [--limit n] [--jobs n]
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const LAB = path.resolve(HERE, '../../../CrystalOSD');
const argv = process.argv.slice(2);
const option = (name) => { const at = argv.indexOf(name); return at >= 0 ? argv[at + 1] : undefined; };
const verifier = path.basename(argv[0] ?? '');
const target = path.basename(option('--target') ?? verifier);
const capture = option('--capture');
if (!verifier || !capture) { console.error('usage: node mutate_here.mjs <verify_x.mjs> [--target file.mjs] --capture name [--env K=V] [--limit n] [--jobs n]'); process.exit(2); }
const LIMIT = Number(option('--limit')) || 60;
const JOBS = Number(option('--jobs')) || Math.max(1, Math.min(8, os.cpus().length - 2));
const env = {};
if (option('--env')) for (const pair of option('--env').split(',')) { const [k, v] = pair.split('='); env[k] = v; }
const CAPTURES = process.env.CRYSTALOSD_CAPTURES ?? path.resolve(HERE, '../../../Watson/Runtime/captures');
const traceFile = path.join(CAPTURES, `${capture}.trace.jsonl`);

const source = fs.readFileSync(path.join(HERE, target), 'utf8');
const lines = source.split('\n');
const skipLine = (line) => /^\s*(import|export const PROBES|\/\/|\*|console\.|if \(process\.argv)/.test(line)
  || /console\.log|throw new Error|problem\(|readFileSync|probe\.pc|pc\(|range\(|pick\(/.test(line)
  || !/\b(f|add|sub|mul|div|sqrt|toInt|toUnsigned|fround|chop|half|Math\.(trunc|floor|min|max|sqrt|sin|cos))\(|\d\.\d|<<|>>|\bBigInt\(/.test(line)
  || /\bfor \(|\.push\(|\+= 1\b|process\.exit|\.slice\(|\.length\b|ArrayBuffer|\.padStart|toFixed/.test(line);
const mutants = [];
let inProbes = false;
lines.forEach((line, i) => {
  if (/^(export )?const (PROBES|A|P) = /.test(line)) inProbes = true;
  if (inProbes) { if (/^\S.*[\]\)};]\s*$/.test(line) || /^\}\);?\s*$/.test(line) || /^\];?\s*$/.test(line)) inProbes = false; return; }
  if (skipLine(line)) return;
  const code = line.replace(/\/\/.*$/, '').replace(/(['"`])(?:\\.|(?!\1).)*\1/g, (m) => ' '.repeat(m.length));
  for (const m of code.matchAll(/(?<![\w.$])(\d+\.\d+(?:e[-+]?\d+)?|0x[0-9a-fA-F]+|\d+)(?![\w.])/g)) {
    const text = m[1];
    let next;
    if (text.startsWith('0x')) { const v = parseInt(text, 16); if (v > 0xffff) continue; next = `0x${(v + 1).toString(16)}`; }
    else if (text.includes('.') || text.includes('e')) next = String(Number(text) * (1 + 1e-6) + (Number(text) === 0 ? 1e-6 : 0));
    else { const v = Number(text); if (v > 0xffff) continue; next = String(v + 1); }
    mutants.push({ line: i, at: m.index, from: text, to: next });
  }
  for (const m of code.matchAll(/(?<=[\w)\]] )(\+|-|\*|<=|>=|<|>)(?= [\w(\[])/g)) {
    const to = { '+': '-', '-': '+', '*': '/', '<': '<=', '<=': '<', '>': '>=', '>=': '>' }[m[1]];
    mutants.push({ line: i, at: m.index, from: m[1], to });
  }
});
const step = Math.max(1, mutants.length / LIMIT);
const chosen = [];
for (let k = 0; k < mutants.length && chosen.length < LIMIT; k += step) chosen.push(mutants[Math.floor(k)]);

// A private copy of this folder's .mjs files with the lab's relative imports made absolute.
function copyWith(dir, mutated) {
  fs.mkdirSync(dir, { recursive: true });
  for (const name of fs.readdirSync(HERE).filter((n) => n.endsWith('.mjs'))) {
    let text = name === target && mutated !== null ? mutated : fs.readFileSync(path.join(HERE, name), 'utf8');
    text = text.replace(/(['"])\.\.\/\.\.\/\.\.\/CrystalOSD\//g, `$1${pathToFileURL(LAB).href}/`);
    fs.writeFileSync(path.join(dir, name), text);
  }
}

function run(dir) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [path.join(dir, verifier), traceFile], { cwd: dir, env: { ...process.env, ...env }, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '';
    child.stdout.on('data', (c) => { out = (out + c).slice(-8000); });
    child.stderr.on('data', (c) => { out = (out + c).slice(-8000); });
    child.on('exit', () => {
      const verdicts = out.split(/\r?\n/).filter((l) => /^verdict:/.test(l.trim()));
      resolve({ found: verdicts.length > 0 && /^verdict: FOUND/.test(verdicts.at(-1).trim()), crashed: verdicts.length === 0 });
    });
  });
}

const work = fs.mkdtempSync(path.join(os.tmpdir(), 'mutate-here-'));
copyWith(path.join(work, 'base'), null);
const base = await run(path.join(work, 'base'));
if (!base.found) { console.error(`the unmutated verifier does not say FOUND on ${capture}`); process.exit(2); }
const results = [];
let next = 0;
const worker = async () => {
  while (next < chosen.length) {
    const index = next++;
    const m = chosen[index];
    const mutated = [...lines];
    mutated[m.line] = mutated[m.line].slice(0, m.at) + m.to + mutated[m.line].slice(m.at + m.from.length);
    const dir = path.join(work, `m${index}`);
    copyWith(dir, mutated.join('\n'));
    try { results[index] = { ...m, ...(await run(dir)) }; } finally { fs.rmSync(dir, { recursive: true, force: true }); }
  }
};
await Promise.all(Array.from({ length: JOBS }, worker));
fs.rmSync(work, { recursive: true, force: true });
const survivors = results.filter((r) => r.found);
const crashed = results.filter((r) => !r.found && r.crashed).length;
console.log(`verifier: ${verifier}   target: ${target}   capture: ${capture}   mutants: ${results.length} of ${mutants.length} candidates`);
for (const s of survivors) console.log(`  survived  line ${s.line + 1}: ${s.from} -> ${s.to}    ${lines[s.line].trim().slice(0, 110)}`);
const killed = results.length - survivors.length;
console.log(`killed ${killed} of ${results.length} (${crashed} by a crash, ${killed - crashed} by a wrong verdict)`);
console.log(`verdict: ${survivors.length ? `PARTIAL ${survivors.length} mutants survive` : 'FOUND every mutant changed the verdict'}`);
process.exit(survivors.length ? 1 : 0);
