import type { Pool } from 'pg';
import { inventory } from './contracts.js';
import type { Session } from './sessions.js';

export async function save(pool: Pool,user: Session,value: unknown) {
  const snapshot=inventory(value,user.device_id);
  await pool.query('INSERT INTO kitlane.snapshots VALUES($1,$2,$3) ON CONFLICT(account_id,device_id) DO UPDATE SET snapshot=$3',[user.account_id,user.device_id,JSON.stringify(snapshot)]);
}
export async function peers(pool: Pool,user: Session) {
  const result=await pool.query(`SELECT s.snapshot FROM kitlane.snapshots s JOIN kitlane.pairs p ON p.account_id=s.account_id
    AND s.device_id=CASE WHEN p.device_a=$2 THEN p.device_b ELSE p.device_a END
    WHERE p.account_id=$1 AND (p.device_a=$2 OR p.device_b=$2)`,[user.account_id,user.device_id]);
  return result.rows.map(r=>r.snapshot);
}
