import { jwtVerify, type JWTVerifyGetKey } from 'jose';
import type { Pool, PoolClient } from 'pg';
import { unauthorized, uuid } from './contracts.js';

export async function verifyIdentity(token: string, keys: JWTVerifyGetKey, config: {issuer: string; audience: string}): Promise<string> {
  try {
    if (token.length > 16384) throw unauthorized();
    const { payload } = await jwtVerify(token, keys, { issuer: config.issuer, audience: config.audience, algorithms: ['ES256','RS256','EdDSA'], requiredClaims: ['sub','iat','exp','iss','aud'], maxTokenAge: '5m', clockTolerance: 5 });
    if (!uuid(payload.sub)) throw unauthorized();
    return payload.sub;
  } catch { throw unauthorized(); }
}

export function googleOnly(accounts: {providerId: string; password: string | null}[]): boolean {
  return accounts.length > 0 && accounts.every(a => a.providerId === 'google' && a.password === null);
}

export type Account = { account_id: string; email: string; name: string };
export async function googleAccount(db: Pool | PoolClient, id: string): Promise<Account> {
  const user = await db.query('SELECT id,email,name,"emailVerified",banned FROM neon_auth."user" WHERE id=$1', [id]);
  const accounts = await db.query('SELECT "providerId",password FROM neon_auth.account WHERE "userId"=$1', [id]);
  const u = user.rows[0];
  if (!u || u.banned || !u.emailVerified || !googleOnly(accounts.rows)) throw unauthorized();
  return {account_id: id, email: u.email, name: u.name};
}
