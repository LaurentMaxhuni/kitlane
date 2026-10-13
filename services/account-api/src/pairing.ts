import type { Pool } from 'pg';
import { ApiError, hash, pairCode, validPairCode } from './contracts.js';
import type { Session } from './sessions.js';
import { transaction } from './db.js';

export async function generate(pool: Pool, user: Session) {
  return transaction(pool, async db => {
    // Lock the device so concurrent generation leaves exactly one current code.
    await db.query('SELECT device_id FROM kitlane.devices WHERE account_id=$1 AND device_id=$2 FOR UPDATE',[user.account_id,user.device_id]);
    await db.query('UPDATE kitlane.pairing_codes SET consumed=true WHERE account_id=$1 AND device_id=$2',[user.account_id,user.device_id]);
    const code=pairCode();
    await db.query(`INSERT INTO kitlane.pairing_codes VALUES($1,$2,$3,now()+interval '5 minutes',false)`,[hash(code),user.account_id,user.device_id]);
    return {code,expires_at:Math.floor(Date.now()/1000)+300};
  });
}
export async function consume(pool: Pool, user: Session, code: unknown) {
  const failure = () => new ApiError(400,'This code is unavailable. Generate a new code on a device signed in to the same Google account.');
  if(!validPairCode(code)) throw failure();
  return transaction(pool, async db => {
    const found=await db.query('UPDATE kitlane.pairing_codes SET consumed=true WHERE code_hash=$1 AND account_id=$2 AND device_id<>$3 AND expires_at>now() AND NOT consumed RETURNING device_id',[hash(code),user.account_id,user.device_id]);
    const peer=found.rows[0]?.device_id;
    if(!peer) throw failure();
    const [a,b]=[user.device_id,peer].sort();
    await db.query('INSERT INTO kitlane.pairs VALUES($1,$2,$3) ON CONFLICT DO NOTHING',[user.account_id,a,b]);
  });
}
export async function devices(pool: Pool,user: Session) {
  const result=await pool.query(`SELECT d.device_id AS id,d.name,d.platform,d.architecture FROM kitlane.pairs p JOIN kitlane.devices d
    ON d.account_id=p.account_id AND d.device_id=CASE WHEN p.device_a=$2 THEN p.device_b ELSE p.device_a END
    WHERE p.account_id=$1 AND (p.device_a=$2 OR p.device_b=$2) ORDER BY d.name,d.device_id`,[user.account_id,user.device_id]);
  return result.rows;
}
