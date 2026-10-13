import {test} from 'node:test';
import assert from 'node:assert/strict';
import {inventory,device} from '../src/contracts.js';

const id='11111111-1111-4111-8111-111111111111';
const snapshot={device:{id,name:'Test computer',platform:'windows',architecture:'x86_64'},scanned_at_unix_secs:100,apps:[{id:'app',name:'An app',publisher:null,version:null,scope:'user',source:'windows_registry'}],warnings:[]};
test('inventories reject device substitution, duplicate apps, local icon data and oversized metadata',()=>{
  assert.equal(inventory(snapshot,id),snapshot);
  assert.throws(()=>inventory(snapshot,'22222222-2222-4222-8222-222222222222'));
  assert.throws(()=>inventory({...snapshot,apps:[...snapshot.apps,...snapshot.apps]},id));
  assert.throws(()=>inventory({...snapshot,apps:[{...snapshot.apps[0],display_icon:'C:\\secret.exe'}]},id));
  assert.throws(()=>inventory({...snapshot,icon_bytes:'data'},id));
  assert.throws(()=>inventory({...snapshot,apps:[{...snapshot.apps[0],name:'a'.repeat(513)}]},id));
  assert.throws(()=>device({...snapshot.device,name:'bad\nname'}));
  assert.throws(()=>inventory({...snapshot,scanned_at_unix_secs:Date.now()},id));
});
