import {test} from 'node:test';
import assert from 'node:assert/strict';
import type {Pool} from 'pg';
import {createApp} from '../src/app.js';

const configuration={origin:'https://kitlane.example',authURL:'https://auth.example/db/auth',jwksURL:'https://auth.example/db/auth/.well-known/jwks.json',issuer:'https://auth.example',audience:'kitlane'};
test('protected endpoints reject missing/invalid credentials before touching database',async()=>{
  let queries=0;
  const pool={query:async()=>{queries++; throw new Error('DATABASE_SECRET_SENTINEL');}} as unknown as Pool;
  const app=createApp(pool,configuration,{html:'',script:''});
  for(const path of ['/v2/account','/v2/devices','/v2/inventory']){
    const response=await app.request(path);
    assert.equal(response.status,401); assert.equal(queries,0);
    assert.doesNotMatch(await response.text(),/SECRET/);
  }
  const complete=await app.request('/v2/auth/complete',{method:'POST',headers:{origin:configuration.origin,authorization:'Bearer invalid','content-type':'application/json'},body:'{}'});
  assert.equal(complete.status,401); assert.equal(queries,0);
});
test('browser completion rejects foreign origins and responses never echo credentials',async()=>{
  const pool={query:async()=>{throw new Error('DATABASE_SECRET_SENTINEL');}} as unknown as Pool;
  const app=createApp(pool,configuration,{html:'',script:''});
  const response=await app.request('/v2/auth/complete',{method:'POST',headers:{origin:'https://foreign.example',authorization:'Bearer CREDENTIAL_SENTINEL'},body:'{}'});
  assert.equal(response.status,403); assert.doesNotMatch(await response.text(),/SENTINEL/);
  assert.equal(response.headers.get('referrer-policy'),'no-referrer');
  assert.equal(response.headers.get('cache-control'),'no-store');
});

test('an abusive installation cannot exhaust another installation sign-in allowance',async()=>{
  const counters=new Map<string,number>();
  const pool={query:async(sql:string,values:unknown[])=>{
    if(sql.includes('INSERT INTO kitlane.rate_limits')){const key=String(values[0]);const count=(counters.get(key)??0)+1;counters.set(key,count);return {rows:[{attempts:count}]};}
    return {rows:[]};
  }} as unknown as Pool;
  const app=createApp(pool,configuration,{html:'',script:''});
  const body={state:'a'.repeat(43),challenge:'b'.repeat(43),device_proof:'c'.repeat(43),callback:'http://127.0.0.1:49152/kitlane/callback',device:{id:'11111111-1111-4111-8111-111111111111',name:'Test',platform:'windows',architecture:'x86_64'}};
  for(let n=0;n<121;n++)await app.request('/v2/auth/attempts',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
  const other=await app.request('/v2/auth/attempts',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({...body,device_proof:'d'.repeat(43)})});
  assert.equal(other.status,200,'Each private installation proof must have its own allowance');
});
