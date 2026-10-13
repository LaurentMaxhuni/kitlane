import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createAuthClient} from '@neondatabase/auth';
import {googleToken} from '../web/google-token.js';

test('bridge uses the official SDK JWT after getSession populates the shared cache',async()=>{
  const fetch=globalThis.fetch;
  const jwt='eyJhbGciOiJFUzI1NiJ9.eyJpc3MiOiJodHRwczovL2F1dGguZXhhbXBsZSJ9.signature';
  const id='11111111-1111-4111-8111-111111111111';
  let requests=0;
  globalThis.fetch=async()=>{
    requests++;
    return Response.json({session:{id,token:'opaque-cookie-secret',userId:id,expiresAt:new Date(Date.now()+3600000).toISOString()},user:{id,name:'Test',email:'test@example.invalid',emailVerified:true}},{headers:{'set-auth-jwt':jwt}});
  };
  try {
    const auth=createAuthClient('https://auth.example/db/auth');
    const current=await auth.getSession();
    assert.ok(current.data?.user);
    assert.equal(await googleToken(auth),jwt);
    assert.equal(requests,1,'Reuse the SDK cached session JWT');
  }finally{globalThis.fetch=fetch;}
});

test('session JWT extraction rejects anonymous, opaque, malformed and oversized credentials',async()=>{
  for(const data of [null,{user:null,session:{token:'a.b.c'}},{user:{},session:{token:'opaque-cookie-secret'}},{user:{},session:{token:'a..b'}},{user:{},session:{token:'a'.repeat(16384)+'.b.c'}}]){
    const auth={getSession:async()=>({data,error:null})} as unknown as Parameters<typeof googleToken>[0];
    await assert.rejects(googleToken(auth));
  }
});
