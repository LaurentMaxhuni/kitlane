import { cp, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { check } from './check.mjs';
import { execFileSync } from 'node:child_process';

await check();
execFileSync(process.execPath, ['--test', 'tests/downloads.test.mjs'], { cwd: fileURLToPath(new URL('../', import.meta.url)), stdio: 'inherit' });
const root = new URL('../', import.meta.url);
const output = new URL('dist/', root);
await mkdir(output, { recursive: true });
for (const name of ['index.html', 'download', 'styles.css', 'site.js', 'assets']) {
  await cp(new URL(name, root), new URL(name, output), { recursive: true });
}
console.log(`Static website built: ${fileURLToPath(output)}`);
