import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';

const base = 'https://github.com/LaurentMaxhuni/kitlane/releases';
function release(version, changes = {}) {
  const names = [`Kitlane-${version}-Setup-x64.exe`, `Kitlane-${version}-macOS-arm64.dmg`, `Kitlane-${version}-macOS-x64.dmg`, `Kitlane-${version}-Linux-x64.tar.gz`, `Kitlane-${version}-Linux-arm64.tar.gz`, 'SHA256SUMS.txt'];
  return { tag_name: `v${version}`, draft: false, prerelease: true, published_at: '2026-10-15T00:00:00Z', assets: names.map(name => ({ name, state: 'uploaded', size: 100, browser_download_url: `${base}/download/v${version}/${name}` })), ...changes };
}

test('page uses permanent download links without a fixed release version', async () => {
  const html = await readFile(new URL('../download/index.html', import.meta.url), 'utf8');
  assert.doesNotMatch(html, /releases\/download\/v\d|Preview \d+\.\d+\.\d+/);
  for (const platform of ['windows-x64', 'macos-arm64', 'macos-x64', 'linux-x64', 'linux-arm64', 'checksums', 'notes']) {
    assert(html.includes(`../api/download?platform=${platform}`), platform);
  }
});

test('selects highest numeric complete version including previews', async () => {
  const { selectRelease } = await import('../lib/releases.js');
  const selected = selectRelease([release('0.9.0'), release('0.10.0'), release('0.2.0'), release('9.0.0', { draft: true }), release('10.0.0', { assets: [] })]);
  assert.equal(selected.version, '0.10.0');
  assert.equal(selected.preview, true);
  assert.equal(selected.downloads['windows-x64'], `${base}/download/v0.10.0/Kitlane-0.10.0-Setup-x64.exe`);
  assert.equal(selected.downloads.notes, `${base}/tag/v0.10.0`);
});

test('rejects incomplete, duplicate, unuploaded, foreign and mismatched assets', async () => {
  const { selectRelease } = await import('../lib/releases.js');
  for (const mutate of [
    r => r.assets.pop(),
    r => r.assets.push(r.assets[0]),
    r => { r.assets[0].state = 'new'; },
    r => { r.assets[0].size = 0; },
    r => { r.assets[0].browser_download_url = 'https://evil.example/installer.exe'; },
    r => { r.assets[0].browser_download_url = `${base}/download/v0.1.0/Kitlane-0.1.0-Setup-x64.exe`; },
    r => { r.published_at = null; },
    r => { r.tag_name = 'v00.2.0'; },
  ]) {
    const candidate = release('0.2.0'); mutate(candidate);
    assert.equal(selectRelease([candidate]), null);
  }
});

test('returns metadata and redirects each platform; rejects unknown platforms and methods', async () => {
  const { createHandler } = await import('../lib/releases.js');
  const handler = createHandler(async () => Response.json([release('0.2.0')]));
  const metadata = await handler(new Request('https://kitlane.test/api/download'));
  assert.equal(metadata.status, 200);
  assert.equal((await metadata.json()).version, '0.2.0');
  for (const platform of ['windows-x64', 'macos-arm64', 'macos-x64', 'linux-x64', 'linux-arm64', 'checksums', 'notes']) {
    const result = await handler(new Request(`https://kitlane.test/api/download?platform=${platform}`));
    assert.equal(result.status, 302);
    assert(result.headers.get('location').startsWith(base + '/'));
    assert.match(result.headers.get('cache-control'), /no-store/);
  }
  assert.equal((await handler(new Request('https://kitlane.test/api/download?platform=toString'))).status, 400);
  assert.equal((await handler(new Request('https://kitlane.test/api/download?platform=windows-x64&platform=notes'))).status, 400);
  assert.equal((await handler(new Request('https://kitlane.test/api/download', { method: 'POST' }))).status, 405);
});

test('caches and coalesces lookups for at most a minute, then sees the newer release', async () => {
  const { createHandler } = await import('../lib/releases.js');
  let now = 0; let calls = 0;
  const handler = createHandler(async () => { calls++; return Response.json([release(calls === 1 ? '0.2.0' : '0.3.0')]); }, () => now);
  const request = () => handler(new Request('https://kitlane.test/api/download'));
  const responses = await Promise.all([request(), request(), request()]);
  assert.equal(calls, 1);
  assert.equal((await responses[0].json()).version, '0.2.0');
  now = 60_001;
  assert.equal((await (await request()).json()).version, '0.3.0');
  assert.equal(calls, 2);
});

test('upstream errors never redirect to stale files or leak response bodies', async () => {
  const { createHandler } = await import('../lib/releases.js');
  for (const fetcher of [async () => new Response('private diagnostic', { status: 403 }), async () => { throw new Error('private diagnostic'); }, async () => Response.json({ invalid: true }), async () => Response.json([])]) {
    const response = await createHandler(fetcher)(new Request('https://kitlane.test/api/download?platform=windows-x64'));
    assert.equal(response.status, 503);
    assert.equal(response.headers.get('location'), null);
    assert.doesNotMatch(await response.text(), /private diagnostic/);
  }
  let now = 0; let calls = 0;
  const handler = createHandler(async () => ++calls === 1 ? Response.json([release('0.2.0')]) : new Response('', { status: 500 }), () => now);
  await handler(new Request('https://kitlane.test/api/download'));
  now = 60_001;
  assert.equal((await handler(new Request('https://kitlane.test/api/download'))).status, 503);
});
