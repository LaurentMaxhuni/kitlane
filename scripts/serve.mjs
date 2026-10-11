import http from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
const port = Number(process.env.PORT || 4173);
const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.ico': 'image/x-icon', '.ttf': 'font/ttf', '.md': 'text/plain' };
http.createServer(async (req, res) => {
  try {
    const pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    const filename = path.resolve(root, '.' + pathname);
    if (filename !== root && !filename.startsWith(root + path.sep)) {
      res.writeHead(403).end('Forbidden'); return;
    }
    const info = await stat(filename);
    const target = info.isDirectory() ? path.join(filename, 'index.html') : filename;
    const data = await readFile(target);
    res.writeHead(200, { 'Content-Type': (types[path.extname(target)] || 'application/octet-stream') + (['.html', '.css', '.js', '.json', '.md'].includes(path.extname(target)) ? '; charset=utf-8' : ''), 'Cache-Control': 'no-cache' });
    res.end(data);
  } catch { res.writeHead(404).end('Not found'); }
}).listen(port, '127.0.0.1', () => console.log(`Kitlane design system: http://localhost:${port}`));
