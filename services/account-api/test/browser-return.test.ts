import {test} from 'node:test';
import assert from 'node:assert/strict';
import {pendingAttempt} from '../web/pending-attempt.js';
import {createApp} from '../src/app.js';
import type {Pool} from 'pg';

test('Google origin-root return retains this tab attempt and renders the bridge',async()=>{
  const configuration={origin:'https://kitlane.example',authURL:'https://auth.example/db/auth',jwksURL:'https://auth.example/db/auth/.well-known/jwks.json',issuer:'https://auth.example',audience:'kitlane'};
  const app=createApp({} as Pool,configuration,{html:'KITLANE_BRIDGE',script:''});
  const response=await app.request('/?neon_auth_session_verifier=test');
  assert.equal(response.status,200);
  assert.equal(await response.text(),'KITLANE_BRIDGE');
  const values=new Map<string,string>();
  const storage={getItem:(key:string)=>values.get(key)??null,setItem:(key:string,value:string)=>{values.set(key,value);},removeItem:(key:string)=>{values.delete(key);}};
  const attempt='a'.repeat(43);
  assert.equal(pendingAttempt(new URL(configuration.origin+'/sign-in?attempt='+attempt),storage,1000),attempt);
  assert.equal(pendingAttempt(new URL(configuration.origin+'/?neon_auth_session_verifier=test'),storage,2000),attempt);
  assert.equal(pendingAttempt(new URL(configuration.origin+'/'),storage,301001),null);
  assert.equal(values.size,0);
  assert.equal(pendingAttempt(new URL(configuration.origin+'/sign-in?attempt=invalid'),storage),null);
  const blocked={...storage,setItem:()=>{throw Error('blocked');}};
  assert.equal(pendingAttempt(new URL(configuration.origin+'/sign-in?attempt='+attempt),blocked),attempt);
});
