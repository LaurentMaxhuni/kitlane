// CI signer. Seed arrives only through a masked GitHub environment secret.
import { createPrivateKey, createPublicKey, createHash, sign } from 'node:crypto';
import { readFile, writeFile, stat } from 'node:fs/promises';
import { basename, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const [installer, version, sequenceText, output] = process.argv.slice(2);
const sequence = Number(sequenceText);
if (!installer || !output || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version ?? '') || !Number.isSafeInteger(sequence) || sequence < 1) {
  throw new Error('Usage: node sign-update.mjs <installer> <version> <positive sequence> <output.json>');
}
if (basename(installer) !== `Kitlane-${version}-Setup-x64.exe`) throw new Error('Incorrect Windows installer filename');
const size = (await stat(installer)).size;
if (size === 0 || size > 512 * 1024 * 1024) throw new Error('Installer size is outside supported limits');
const encoded = process.env.KITLANE_UPDATE_SIGNING_KEY;
if (!encoded || !/^[A-Za-z0-9+/]{43}=$/.test(encoded)) throw new Error('KITLANE_UPDATE_SIGNING_KEY must contain a base64 32-byte Ed25519 seed');
const seed = Buffer.from(encoded, 'base64');
if (seed.length !== 32) throw new Error('Invalid signing seed size');
const der = Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]);
const privateKey = createPrivateKey({ key: der, format: 'der', type: 'pkcs8' });
seed.fill(0);
der.fill(0);
const publicDer = createPublicKey(privateKey).export({ format: 'der', type: 'spki' });
const publicKey = publicDer.subarray(-32).toString('base64');
const root = JSON.parse(await readFile(resolve(dirname(fileURLToPath(import.meta.url)), '../update-root.json'), 'utf8'));
const trusted = root.keys.find(key => key.public_key === publicKey && !root.revoked_key_ids.includes(key.id));
if (root.schema !== 1 || !trusted || sequence < root.minimum_sequence) throw new Error('Signing key or sequence is not authorized by bundled update root');
const now = Math.floor(Date.now() / 1000);
const payload = Buffer.from(JSON.stringify({ schema: 1, repository: 'LaurentMaxhuni/kitlane', channel: 'preview', sequence, version, published_at: now, expires_at: now + 30 * 24 * 60 * 60, os: 'windows', arch: 'x86_64', size, sha256: createHash('sha256').update(await readFile(installer)).digest('hex') }));
const signature = sign(null, Buffer.concat([Buffer.from('kitlane-update-v1\0'), payload]), privateKey);
await writeFile(output, JSON.stringify({ key_id: trusted.id, payload: payload.toString('base64'), signature: signature.toString('base64') }, null, 2) + '\n', { flag: 'wx' });
console.log(`Signed Kitlane ${version}, sequence ${sequence}`);
