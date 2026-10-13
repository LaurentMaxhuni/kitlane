import {test} from 'node:test';
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {Pool} from 'pg';
import {generate,consume,devices} from '../src/pairing.js';
import {attempt,complete,exchange} from '../src/handoff.js';
import {refresh,session,revoke,type Session} from '../src/sessions.js';
import {save,peers} from '../src/inventory.js';
import {challenge,secret,hash} from '../src/contracts.js';

test('database: transactional handoffs, refresh rotation, pairing races, and account isolation', {skip:!process.env.TEST_DATABASE_URL},async()=>{
  assert.equal(process.env.KITLANE_TEST_BRANCH,'br-falling-sun-barbvxtf','Use only the isolated account-tests branch');
  assert.equal(new URL(process.env.TEST_DATABASE_URL!).hostname,'ep-proud-shadow-badztdco-pooler.c-8.eu-central-1.aws.neon.tech','Refuse to create test fixtures on any other endpoint');
  const pool=new Pool({connectionString:process.env.TEST_DATABASE_URL,max:5,connectionTimeoutMillis:10000});
  pool.on('error',()=>{});
  const accountA=randomUUID(),accountB=randomUUID();
  const d1={id:randomUUID(),name:'Test 1',platform:'windows' as const,architecture:'x86_64'};
  const d2={...d1,id:randomUUID(),name:'Test 2'};
  const d3={...d1,id:randomUUID(),name:'Foreign'};
  const proofs=new Map([d1,d2,d3].map(d=>[d.id,secret()]));
  try{
    for(const account of [accountA,accountB]){
      await pool.query('INSERT INTO neon_auth."user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES($1,$2,$3,true,now(),now())',[account,'Kitlane isolated test',account+'@test.invalid']);
      await pool.query('INSERT INTO neon_auth.account(id,"accountId","providerId","userId","createdAt","updatedAt") VALUES($1,$2,$3,$4,now(),now())',[randomUUID(),account,'google',account]);
      await pool.query('INSERT INTO kitlane.accounts VALUES($1)',[account]);
    }
    for(const [a,d] of [[accountA,d1],[accountA,d2],[accountB,d3]] as const)await pool.query('INSERT INTO kitlane.devices(account_id,device_id,name,platform,architecture,installation_hash) VALUES($1,$2,$3,$4,$5,$6)',[a,d.id,d.name,d.platform,d.architecture,hash(proofs.get(d.id)!)]);
    const user=(a:string,d:string):Session=>({account_id:a,device_id:d,session_id:randomUUID(),account:{account_id:a,email:a+'@test.invalid',name:'Test'}});
    const a=user(accountA,d1.id),b=user(accountA,d2.id),foreign=user(accountB,d3.id);
    const code=await generate(pool,a);
    await assert.rejects(consume(pool,foreign,code.code));
    await assert.rejects(consume(pool,a,code.code));
    const raced=await Promise.allSettled([consume(pool,b,code.code),consume(pool,b,code.code)]);
    assert.equal(raced.filter(r=>r.status==='fulfilled').length,1);
    assert.equal((await devices(pool,b)).length,1);assert.equal((await devices(pool,foreign)).length,0);
    const expired=await generate(pool,a);
    await pool.query("UPDATE kitlane.pairing_codes SET expires_at=now()-interval '1 second' WHERE code_hash=$1",[hash(expired.code)]);
    await assert.rejects(consume(pool,b,expired.code));
    const local={device:d1,scanned_at_unix_secs:100,apps:[],warnings:[]};
    assert.equal((await peers(pool,b)).length,0,'pairing never uploads');
    await save(pool,a,local);assert.equal((await peers(pool,b)).length,1);assert.equal((await peers(pool,foreign)).length,0);
    await assert.rejects(save(pool,foreign,local));
    await pool.query('DELETE FROM kitlane.pairs WHERE account_id=$1',[accountA]);
    assert.equal((await peers(pool,b)).length,0,'unpaired snapshots inaccessible');
    const verifier=secret(),state=secret();
    const maliciousVerifier=secret();
    const stolen=await attempt(pool,{device:d1,device_proof:secret(),state:secret(),challenge:challenge(maliciousVerifier),callback:'http://127.0.0.1:49152/kitlane/callback'},'https://kitlane.example');
    const stolenId=new URL(stolen.browser_url).searchParams.get('attempt')!;
    const stolenFinish=await complete(pool,stolenId,accountA);
    await assert.rejects(exchange(pool,{code:new URL(stolenFinish.callback).searchParams.get('code'),verifier:maliciousVerifier,device_id:d1.id}),'new installation must not adopt an existing paired UUID');
    const pending=await attempt(pool,{device:d1,device_proof:proofs.get(d1.id),state,challenge:challenge(verifier),callback:'http://127.0.0.1:49152/kitlane/callback'},'https://kitlane.example');
    const id=new URL(pending.browser_url).searchParams.get('attempt')!;
    const finish=await complete(pool,id,accountA);
    const redirect=new URL(finish.callback);assert.equal(redirect.searchParams.get('state'),state);
    const body={code:redirect.searchParams.get('code'),verifier,device_id:d1.id};
    await assert.rejects(exchange(pool,{...body,verifier:secret()}));
    await assert.rejects(exchange(pool,{...body,device_id:d2.id}));
    const exchanges=await Promise.allSettled([exchange(pool,body),exchange(pool,body)]);
    assert.equal(exchanges.filter(r=>r.status==='fulfilled').length,1);
    const issued=exchanges.find(r=>r.status==='fulfilled')! as PromiseFulfilledResult<Awaited<ReturnType<typeof exchange>>>;
    const old=issued.value;
    assert.equal((await session(pool,old.access_token)).device_id,d1.id);
    const rotations=await Promise.allSettled([refresh(pool,old.refresh_token,d1.id),refresh(pool,old.refresh_token,d1.id)]);
    assert.equal(rotations.filter(r=>r.status==='fulfilled').length,1);
    await assert.rejects(session(pool,old.access_token));
    const rotated=rotations.find(r=>r.status==='fulfilled')! as PromiseFulfilledResult<Awaited<ReturnType<typeof refresh>>>;
    await revoke(pool,rotated.value.refresh_token,d2.id);
    assert.equal((await session(pool,rotated.value.access_token)).device_id,d1.id,'wrong device cannot revoke');
    await pool.query("UPDATE kitlane.sessions SET access_expires_at=now()-interval '1 second' WHERE access_hash=$1",[hash(rotated.value.access_token)]);
    await revoke(pool,rotated.value.refresh_token,d1.id);
    await assert.rejects(session(pool,rotated.value.access_token));
    await assert.rejects(refresh(pool,rotated.value.refresh_token,d1.id));
  }finally{
    for(const a of [accountA,accountB])await pool.query('DELETE FROM neon_auth."user" WHERE id=$1 AND email=$2',[a,a+'@test.invalid']);
    await pool.end();
  }
});
