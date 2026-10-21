// Copies Ruffle's self-hosted player from node_modules into web/ruffle.
import { cpSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const src = join('node_modules', '@ruffle-rs', 'ruffle');
const dst = join('web', 'ruffle');
rmSync(dst, { recursive: true, force: true });
mkdirSync(dst, { recursive: true });
for (const f of readdirSync(src)) {
    if (!f.endsWith('.map') && f !== 'package.json' && f !== 'README.md') cpSync(join(src, f), join(dst, f));
}
