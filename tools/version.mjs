// Prints the next release version, 0.<phase>.<n>: the phase is the minor number in
// tauri.conf.json, and n counts up from the highest existing v0.<phase>.* tag.
import { readFileSync, appendFileSync } from 'node:fs';
import { execSync } from 'node:child_process';

const conf = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
const phase = Number(conf.version.split('.')[1]);
const tags = execSync(`git tag -l "v0.${phase}.*"`, { encoding: 'utf8' }).split('\n').filter(Boolean);
const used = tags.map((t) => Number(t.split('.')[2])).filter(Number.isInteger);
const next = used.length ? Math.max(...used) + 1 : 0;
const version = `0.${phase}.${next}`;

if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\n`);
console.log(JSON.stringify({ version }));
