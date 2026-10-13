import type { Pool } from 'pg';
import { callbackURL, challenge, device, equal, hash, invalid, opaque, secret, unauthorized } from './contracts.js';
import { googleAccount } from './auth.js';
import { transaction } from './db.js';
import { issue } from './sessions.js';
import {limit} from './limits.js';

export async function attempt(pool: Pool, body: Record<string,unknown>, origin: string) {
  if (!opaque(body.state) || !opaque(body.challenge) || !opaque(body.device_proof)) throw invalid();
  const d = device(body.device), callback = callbackURL(body.callback);
  const installationHash=hash(body.device_proof);
  await limit(pool,`handoff:${installationHash}`,10);
  // Bound retained expired public-attempt/quota rows without logging their contents.
  await pool.query("DELETE FROM kitlane.handoffs WHERE attempt_hash IN (SELECT attempt_hash FROM kitlane.handoffs WHERE expires_at<now() ORDER BY expires_at LIMIT 200)");
  await pool.query("DELETE FROM kitlane.rate_limits WHERE key_hash IN (SELECT key_hash FROM kitlane.rate_limits WHERE starts_at<now()-interval '1 hour' ORDER BY starts_at LIMIT 200)");
  const id = secret();
  await pool.query(`INSERT INTO kitlane.handoffs(attempt_hash,state,challenge,callback,device,expires_at,installation_hash) VALUES($1,$2,$3,$4,$5,now()+interval '5 minutes',$6)`,[hash(id),body.state,body.challenge,callback,JSON.stringify(d),installationHash]);
  return {browser_url:`${origin}/sign-in?attempt=${id}`,expires_at:Math.floor(Date.now()/1000)+300};
}
export async function complete(pool: Pool, id: unknown, subject: string) {
  if (!opaque(id)) throw invalid();
  return transaction(pool, async db => {
    const account = await googleAccount(db,subject);
    const found = await db.query('SELECT * FROM kitlane.handoffs WHERE attempt_hash=$1 AND expires_at>now() AND NOT completed AND NOT consumed FOR UPDATE',[hash(id)]);
    const row=found.rows[0];
    if(!row) throw unauthorized();
    await db.query('INSERT INTO kitlane.accounts VALUES($1) ON CONFLICT DO NOTHING',[account.account_id]);
    const code=secret();
    await db.query('UPDATE kitlane.handoffs SET code_hash=$1,account_id=$2,completed=true WHERE attempt_hash=$3',[hash(code),account.account_id,hash(id)]);
    const callback=new URL(row.callback);
    callback.searchParams.set('code',code); callback.searchParams.set('state',row.state);
    return {callback:callback.href};
  });
}
export async function exchange(pool: Pool, body: Record<string,unknown>) {
  if (!opaque(body.code) || !opaque(body.verifier)) throw unauthorized();
  const known=await pool.query('SELECT attempt_hash FROM kitlane.handoffs WHERE code_hash=$1 AND expires_at>now() AND completed AND NOT consumed',[hash(body.code)]);
  if(!known.rows[0])throw unauthorized();
  await limit(pool,`exchange:${known.rows[0].attempt_hash}`,10);
  return transaction(pool, async db => {
    const found=await db.query('SELECT * FROM kitlane.handoffs WHERE code_hash=$1 AND expires_at>now() AND completed AND NOT consumed FOR UPDATE',[hash(body.code as string)]);
    const row=found.rows[0];
    if(!row || !equal(row.challenge,challenge(body.verifier as string)) || row.device.id!==body.device_id) throw unauthorized();
    const account=await googleAccount(db,row.account_id);
    await db.query('UPDATE kitlane.handoffs SET consumed=true WHERE attempt_hash=$1',[row.attempt_hash]);
    return issue(db,account,device(row.device),row.installation_hash);
  });
}
