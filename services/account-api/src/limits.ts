import type { Pool } from 'pg';
import { ApiError, hash } from './contracts.js';

export async function limit(pool: Pool, key: string, maximum: number, seconds = 60) {
  const result = await pool.query(`INSERT INTO kitlane.rate_limits(key_hash,starts_at,attempts) VALUES($1,now(),1)
    ON CONFLICT(key_hash) DO UPDATE SET
      attempts=CASE WHEN kitlane.rate_limits.starts_at < now()-($2 * interval '1 second') THEN 1 ELSE kitlane.rate_limits.attempts+1 END,
      starts_at=CASE WHEN kitlane.rate_limits.starts_at < now()-($2 * interval '1 second') THEN now() ELSE kitlane.rate_limits.starts_at END
    RETURNING attempts`, [hash(key), seconds]);
  if (result.rows[0].attempts > maximum) throw new ApiError(429,'Too many attempts. Try again later.');
}
