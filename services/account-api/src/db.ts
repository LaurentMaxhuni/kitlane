import type { Pool, PoolClient } from 'pg';

export async function transaction<T>(pool: Pool, operation: (db: PoolClient) => Promise<T>): Promise<T> {
  const db = await pool.connect();
  try { await db.query('BEGIN'); const value = await operation(db); await db.query('COMMIT'); return value; }
  catch (error) { await db.query('ROLLBACK'); throw error; }
  finally { db.release(); }
}
