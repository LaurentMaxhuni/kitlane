import { createHash, randomBytes, randomInt, timingSafeEqual } from 'node:crypto';

export class ApiError extends Error {
  constructor(public readonly status: 400 | 401 | 403 | 409 | 429, message: string) { super(message); }
}
export const invalid = () => new ApiError(400, 'Invalid request.');
export const unauthorized = () => new ApiError(401, 'Sign in with Google again.');
export const hash = (value: string) => createHash('sha256').update(value).digest('hex');
export const challenge = (value: string) => createHash('sha256').update(value).digest('base64url');
export const secret = () => randomBytes(32).toString('base64url');
export const uuid = (value: unknown): value is string => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
export const text = (value: unknown, max: number): value is string => typeof value === 'string' && value.trim().length > 0 && Buffer.byteLength(value) <= max && !/[\x00-\x1f\x7f]/.test(value);
export const opaque = (value: unknown): value is string => typeof value === 'string' && /^[A-Za-z0-9_-]{43}$/.test(value);
export const equal = (a: string, b: string) => a.length === b.length && timingSafeEqual(Buffer.from(a), Buffer.from(b));
export const pairAlphabet = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
export const pairCode = () => Array.from({length: 8}, () => pairAlphabet[randomInt(pairAlphabet.length)]).join('');
export const validPairCode = (value: unknown): value is string => typeof value === 'string' && /^[ABCDEFGHJKLMNPQRSTUVWXYZ23456789]{8}$/.test(value);

export function callbackURL(value: unknown): string {
  if (typeof value !== 'string' || value.length > 128) throw invalid();
  const url = new URL(value);
  const port = Number(url.port);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || port < 1024 || port > 65535 || url.username || url.password || url.pathname !== '/kitlane/callback' || url.search || url.hash || value !== url.href) throw invalid();
  return url.href;
}

export type Device = { id: string; name: string; platform: 'windows' | 'macos' | 'linux'; architecture: string };
export function device(value: unknown): Device {
  if (!value || typeof value !== 'object') throw invalid();
  const d = value as Record<string, unknown>;
  if (Object.keys(d).some(k => !['id','name','platform','architecture'].includes(k)) || !uuid(d.id) || !text(d.name,128) || !text(d.architecture,32) || !['windows','macos','linux'].includes(String(d.platform))) throw invalid();
  return d as Device;
}

export function inventory(value: unknown, owner: string) {
  if (!value || typeof value !== 'object') throw invalid();
  const s = value as Record<string, unknown>;
  if (Object.keys(s).some(k => !['device','scanned_at_unix_secs','apps','warnings'].includes(k)) || device(s.device).id !== owner || !Number.isSafeInteger(s.scanned_at_unix_secs) || Number(s.scanned_at_unix_secs) < 0 || Number(s.scanned_at_unix_secs) > Date.now()/1000+300 || !Array.isArray(s.apps) || s.apps.length > 10000 || !Array.isArray(s.warnings) || s.warnings.length > 64 || !s.warnings.every(w => text(w,1024))) throw invalid();
  const ids = new Set<string>();
  for (const a of s.apps) {
    if (!a || typeof a !== 'object' || Object.keys(a).some(k => !['id','name','publisher','version','scope','source','native_id','origin'].includes(k)) || !text(a.id,256) || !text(a.name,512) || ids.has(a.id) || !['user','machine','unknown'].includes(a.scope) || !['windows_registry','macos_bundle','flatpak','manual'].includes(a.source)) throw invalid();
    for (const [field,max] of [['publisher',512],['version',128],['native_id',256],['origin',256]] as const) if (a[field] != null && !text(a[field],max)) throw invalid();
    ids.add(a.id);
  }
  return s;
}
