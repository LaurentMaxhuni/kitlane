import { randomUUID } from 'node:crypto';
import type { Pool, PoolClient } from 'pg';
import { ApiError,hash, opaque, secret, unauthorized, type Device } from './contracts.js';
import { googleAccount, type Account } from './auth.js';
import { transaction } from './db.js';
import {limit} from './limits.js';

export type Session = { account_id: string; device_id: string; session_id: string; account: Account };
export async function issue(db: PoolClient, account: Account, d: Device, installationHash: string) {
  if(!/^[0-9a-f]{64}$/.test(installationHash??''))throw unauthorized();
  const access_token = secret(), refresh_token = secret();
  const registration=await db.query('INSERT INTO kitlane.devices(account_id,device_id,name,platform,architecture,installation_hash) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(account_id,device_id) DO UPDATE SET name=$3,platform=$4,architecture=$5 WHERE kitlane.devices.installation_hash=$6 RETURNING device_id',[account.account_id,d.id,d.name,d.platform,d.architecture,installationHash]);
  if(!registration.rows[0])throw new ApiError(409,'Device ownership proof is unavailable. Restore this installation’s OS credentials before signing in.');
  await db.query(`INSERT INTO kitlane.sessions VALUES($1,$2,$3,$4,$5,now()+interval '15 minutes',now()+interval '30 days',false)`, [randomUUID(), account.account_id, d.id, hash(access_token), hash(refresh_token)]);
  return {access_token,refresh_token,account,device_id:d.id,expires_at:Math.floor(Date.now()/1000)+900};
}
export async function session(pool: Pool, token: unknown): Promise<Session> {
  if (!opaque(token)) throw unauthorized();
  const result = await pool.query('SELECT account_id,device_id,session_id FROM kitlane.sessions WHERE access_hash=$1 AND access_expires_at>now() AND NOT revoked', [hash(token)]);
  const row = result.rows[0];
  if (!row) throw unauthorized();
  return {...row, account: await googleAccount(pool,row.account_id)};
}
export async function refresh(pool: Pool, token: unknown, deviceId: unknown) {
  if (!opaque(token)) throw unauthorized();
  const known=await pool.query('SELECT session_id FROM kitlane.sessions WHERE refresh_hash=$1 AND refresh_expires_at>now() AND NOT revoked',[hash(token)]);
  if(!known.rows[0])throw unauthorized();
  await limit(pool,`refresh:${known.rows[0].session_id}`,10);
  return transaction(pool, async db => {
    const result = await db.query('SELECT * FROM kitlane.sessions WHERE refresh_hash=$1 AND refresh_expires_at>now() AND NOT revoked FOR UPDATE',[hash(token)]);
    const row = result.rows[0];
    if (!row || row.device_id !== deviceId) throw unauthorized();
    const account = await googleAccount(db,row.account_id);
    const access_token=secret(),refresh_token=secret();
    await db.query(`UPDATE kitlane.sessions SET access_hash=$1,refresh_hash=$2,access_expires_at=now()+interval '15 minutes' WHERE session_id=$3`,[hash(access_token),hash(refresh_token),row.session_id]);
    return {access_token,refresh_token,account,device_id:row.device_id,expires_at:Math.floor(Date.now()/1000)+900};
  });
}

export async function revoke(pool:Pool,token:unknown,deviceId:unknown){
  if(!opaque(token))throw unauthorized();
  await pool.query('UPDATE kitlane.sessions SET revoked=true WHERE refresh_hash=$1 AND device_id=$2',[hash(token),deviceId]);
}
