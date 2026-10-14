import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = new URL('../', import.meta.url);
export async function check() {
  let references = 0;
  for (const name of ['index.html', 'download/index.html']) {
    const file = new URL(name, root);
    const html = await readFile(file, 'utf8');
    assert.equal((html.match(/<h1[ >]/g) || []).length, 1, `${name}: expected one main heading`);
    assert.match(html, /<html lang="en">/, `${name}: document language missing`);
    const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
    assert.equal(new Set(ids).size, ids.length, `${name}: duplicate element IDs`);
    for (const [, reference] of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
      if (/^https:\/\//.test(reference)) continue;
      const target = new URL(reference, file);
      assert(target.pathname.startsWith(root.pathname), `${name}: reference outside website: ${reference}`);
      let targetFile = new URL(target.href);
      targetFile.hash = '';
      const info = await stat(targetFile);
      if (info.isDirectory()) targetFile = new URL('index.html', targetFile.href.endsWith('/') ? targetFile : `${targetFile.href}/`);
      await stat(targetFile);
      if (target.hash) {
        const document = await readFile(targetFile, 'utf8');
        assert(document.includes(`id="${target.hash.slice(1)}"`), `${name}: missing anchor ${reference}`);
      }
      references++;
    }
  }
  const css = await readFile(new URL('styles.css', root), 'utf8');
  for (const [, reference] of css.matchAll(/url\('([^']+)'\)/g)) { await stat(new URL(reference, root)); references++; }
  for (const name of ['library.png', 'devices.png']) await stat(new URL(`assets/${name}`, root));
  for (const license of ['fonts/Inter-OFL.txt', 'fonts/Sora-OFL.txt', 'icons/LICENSE', 'programs/NOTICE.md']) await stat(new URL(`assets/${license}`, root));
  console.log(`Website check passed: 2 pages, ${references} local references, preview images, and asset licenses.`);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await check();
