// Copies Ruffle's self-hosted player from node_modules into web/ruffle.
import { cpSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const src = join(root, 'node_modules', '@ruffle-rs', 'ruffle');
const dst = join(root, 'web', 'ruffle');
rmSync(dst, { recursive: true, force: true });
mkdirSync(dst, { recursive: true });
for (const f of readdirSync(src)) {
    if (!f.endsWith('.map') && f !== 'package.json' && f !== 'README.md') cpSync(join(src, f), join(dst, f));
}
