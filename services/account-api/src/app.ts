import {Hono} from 'hono';
import {bodyLimit} from 'hono/body-limit';
import {createRemoteJWKSet} from 'jose';
import type {Pool} from 'pg';
import type {Config} from './config.js';
import {ApiError,invalid,uuid,opaque,hash,unauthorized,device} from './contracts.js';
import {verifyIdentity} from './auth.js';
import {attempt,complete,exchange} from './handoff.js';
import {session,refresh,revoke,type Session} from './sessions.js';
import {generate,consume,devices} from './pairing.js';
import {save,peers} from './inventory.js';
import {limit} from './limits.js';

export function createApp(pool:Pool,config:Config,assets:{html:string;script:string}) {
  const app=new Hono<{Variables:{session:Session}}>();
  const keys=createRemoteJWKSet(new URL(config.jwksURL),{timeoutDuration:5000});
  const bearer=(header:string|undefined)=>header?.startsWith('Bearer ')?header.slice(7):undefined;
  app.use('*',async(c,next)=>{
    c.header('Cache-Control','no-store'); c.header('Referrer-Policy','no-referrer');
    c.header('X-Content-Type-Options','nosniff');
    c.header('Content-Security-Policy',`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' ${new URL(config.authURL).origin}; frame-src ${new URL(config.authURL).origin}; img-src 'self' data:; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`);
    await next();
  });
  app.onError((error,c)=>error instanceof ApiError ? c.json({error:error.message},error.status) : c.json({error:'Account service is temporarily unavailable.'},503));
  app.get('/health',c=>c.json({ok:true}));
  app.get('/',c=>c.html(assets.html));
  app.get('/sign-in',c=>c.html(assets.html));
  app.get('/bridge.js',c=>{c.header('Content-Type','text/javascript; charset=utf-8');return c.body(assets.script);});
  app.get('/v2/config',c=>c.json({auth_url:config.authURL}));
  app.use('/v2/*',bodyLimit({maxSize:4*1024*1024,onError:c=>c.json({error:'Request exceeds allowed size.'},413)}));
  const json=async(c:{req:{header:(name:string)=>string|undefined;json:()=>Promise<unknown>}})=>{
    if(c.req.header('content-type')?.split(';')[0]!=='application/json') throw invalid();
    try {const body=await c.req.json(); if(!body||typeof body!=='object'||Array.isArray(body))throw invalid();return body as Record<string,unknown>;}catch{throw invalid();}
  };
  app.post('/v2/auth/attempts',async c=>{
    return c.json(await attempt(pool,await json(c),config.origin));
  });
  app.get('/v2/auth/attempts/:id',async c=>{
    const id=c.req.param('id'); if(!opaque(id))throw unauthorized();
    const found=await pool.query('SELECT device FROM kitlane.handoffs WHERE attempt_hash=$1 AND expires_at>now() AND NOT completed AND NOT consumed',[hash(id)]);
    if(!found.rows[0])throw unauthorized();
    return c.json({device:found.rows[0].device});
  });
  app.post('/v2/auth/complete',async c=>{
    if(c.req.header('origin')!==config.origin)throw new ApiError(403,'Invalid browser origin.');
    const token=bearer(c.req.header('authorization'));if(!token)throw unauthorized();
    const subject=await verifyIdentity(token,keys,config);
    await limit(pool,`complete:${subject}`,10);
    return c.json(await complete(pool,(await json(c)).attempt,subject));
  });
  app.post('/v2/auth/exchange',async c=>{
    return c.json(await exchange(pool,await json(c)));
  });
  app.post('/v2/auth/refresh',async c=>{
    const body=await json(c);
    if(!uuid(body.device_id)||!opaque(body.refresh_token))throw unauthorized();
    return c.json(await refresh(pool,body.refresh_token,body.device_id));
  });
  app.post('/v2/auth/revoke',async c=>{
    const body=await json(c);if(!uuid(body.device_id))throw unauthorized();
    await revoke(pool,body.refresh_token,body.device_id);return c.json({ok:true});
  });
  app.use('/v2/*',async(c,next)=>{c.set('session',await session(pool,bearer(c.req.header('authorization'))));await next();});
  app.get('/v2/account',c=>{const u=c.get('session');return c.json({account:u.account,device_id:u.device_id});});
  app.put('/v2/device',async c=>{
    const d=device(await json(c)),u=c.get('session');if(d.id!==u.device_id)throw new ApiError(403,'Invalid device identity.');
    await pool.query('UPDATE kitlane.devices SET name=$1,platform=$2,architecture=$3 WHERE account_id=$4 AND device_id=$5',[d.name,d.platform,d.architecture,u.account_id,u.device_id]);
    return c.json({ok:true});
  });
  app.post('/v2/auth/logout',async c=>{
    await pool.query('UPDATE kitlane.sessions SET revoked=true WHERE session_id=$1',[c.get('session').session_id]);
    return c.json({ok:true});
  });
  app.post('/v2/pairing/codes',async c=>{const u=c.get('session');await limit(pool,`codes:${u.account_id}:${u.device_id}`,5);return c.json(await generate(pool,u));});
  app.post('/v2/pairing/consume',async c=>{const u=c.get('session');await limit(pool,`guess:${u.account_id}:${u.device_id}`,5);await limit(pool,`guesses:${u.account_id}`,30,3600);await consume(pool,u,(await json(c)).code);return c.json({devices:await devices(pool,u)});});
  app.get('/v2/devices',async c=>c.json({devices:await devices(pool,c.get('session'))}));
  app.delete('/v2/devices/:id',async c=>{
    const id=c.req.param('id'),u=c.get('session');if(!uuid(id))throw invalid();
    const [a,b]=[id,u.device_id].sort();
    await pool.query('DELETE FROM kitlane.pairs WHERE account_id=$1 AND device_a=$2 AND device_b=$3',[u.account_id,a,b]);
    return c.json({ok:true});
  });
  app.put('/v2/inventory',async c=>{const u=c.get('session');await limit(pool,`sync:${u.account_id}:${u.device_id}`,10);await save(pool,u,await json(c));return c.json({ok:true});});
  app.get('/v2/inventory',async c=>c.json({peers:await peers(pool,c.get('session'))}));
  app.notFound(c=>c.json({error:'Not found.'},404));
  return app;
}
