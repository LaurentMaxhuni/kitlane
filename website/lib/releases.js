const repository = 'https://github.com/LaurentMaxhuni/kitlane';
const platforms = {
  'windows-x64': version => `Kitlane-${version}-Setup-x64.exe`,
  'macos-arm64': version => `Kitlane-${version}-macOS-arm64.dmg`,
  'macos-x64': version => `Kitlane-${version}-macOS-x64.dmg`,
  'linux-x64': version => `Kitlane-${version}-Linux-x64.tar.gz`,
  'linux-arm64': version => `Kitlane-${version}-Linux-arm64.tar.gz`,
  checksums: () => 'SHA256SUMS.txt',
};

export function selectRelease(releases) {
  if (!Array.isArray(releases)) throw new Error('Invalid releases response');
  const candidates = releases.filter(release => release && !release.draft && release.published_at && /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(release.tag_name) && release.tag_name.length < 64);
  candidates.sort((a, b) => {
    const left = a.tag_name.slice(1).split('.').map(BigInt);
    const right = b.tag_name.slice(1).split('.').map(BigInt);
    for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] > right[i] ? -1 : 1;
    return 0;
  });
  for (const release of candidates) {
    if (!Array.isArray(release.assets)) continue;
    const version = release.tag_name.slice(1);
    const downloads = {};
    for (const [platform, filename] of Object.entries(platforms)) {
      const name = filename(version);
      const assets = release.assets.filter(asset => asset?.name === name);
      const url = `${repository}/releases/download/${release.tag_name}/${name}`;
      if (assets.length !== 1 || assets[0].state !== 'uploaded' || !(assets[0].size > 0) || assets[0].browser_download_url !== url) break;
      downloads[platform] = url;
    }
    if (Object.keys(downloads).length !== Object.keys(platforms).length) continue;
    downloads.notes = `${repository}/releases/tag/${release.tag_name}`;
    return { version, preview: release.prerelease === true, downloads };
  }
  return null;
}

// A single shared lookup per warm instance/minute, including concurrent requests.
// Responses themselves cannot be cached, so redirects never become permanent.
export function createHandler(fetcher = fetch, now = Date.now) {
  let cached;
  let expiresAt = 0;
  let pending;
  async function latest() {
    if (cached && now() < expiresAt) return cached;
    if (!pending) {
      pending = (async () => {
        const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'Kitlane-downloads', 'X-GitHub-Api-Version': '2026-03-10' };
        if (process.env.GITHUB_RELEASES_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_RELEASES_TOKEN}`;
        const response = await fetcher('https://api.github.com/repos/LaurentMaxhuni/kitlane/releases?per_page=100', { headers, signal: AbortSignal.timeout(10_000), redirect: 'error', cache: 'no-store' });
        if (!response.ok) throw new Error('Release lookup failed');
        const release = selectRelease(await response.json());
        if (!release) throw new Error('No complete release');
        cached = release;
        expiresAt = now() + 60_000;
        return release;
      })();
    }
    try { return await pending; } finally { pending = undefined; }
  }
  return async request => {
    const headers = { 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' };
    if (!['GET', 'HEAD'].includes(request.method)) return new Response(null, { status: 405, headers: { ...headers, Allow: 'GET, HEAD' } });
    const params = new URL(request.url).searchParams;
    const platform = params.get('platform');
    if (params.getAll('platform').length > 1 || (platform !== null && !Object.hasOwn(platforms, platform) && platform !== 'notes')) {
      return new Response('Unknown download platform.', { status: 400, headers });
    }
    try {
      const release = await latest();
      if (platform !== null) return new Response(null, { status: 302, headers: { ...headers, Location: release.downloads[platform] } });
      return new Response(request.method === 'HEAD' ? null : JSON.stringify(release), { headers: { ...headers, 'Content-Type': 'application/json; charset=utf-8' } });
    } catch {
      return new Response(request.method === 'HEAD' ? null : 'Downloads are temporarily unavailable. Please try again shortly or visit https://github.com/LaurentMaxhuni/kitlane/releases.', { status: 503, headers: { ...headers, 'Content-Type': 'text/plain; charset=utf-8', 'Retry-After': '60' } });
    }
  };
}
