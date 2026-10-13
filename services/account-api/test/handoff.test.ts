import { test } from 'node:test';
import assert from 'node:assert/strict';
import { callbackURL, challenge, secret, hash, validPairCode } from '../src/contracts.js';

test('callback accepts bounded exact loopback path only', () => {
  assert.equal(callbackURL('http://127.0.0.1:49152/kitlane/callback'), 'http://127.0.0.1:49152/kitlane/callback');
  for (const value of ['https://evil.example/callback', 'http://localhost:49152/kitlane/callback', 'http://127.0.0.1.evil.test:49152/kitlane/callback', 'http://user@127.0.0.1:49152/kitlane/callback', 'http://127.0.0.1:80/kitlane/callback', 'http://127.0.0.1:49152/elsewhere', 'http://127.0.0.1:49152/kitlane/callback?token=x']) {
    assert.throws(() => callbackURL(value));
  }
});

test('PKCE uses SHA256 and secrets have sufficient entropy', () => {
  assert.equal(challenge('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk'), 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM');
  const one = secret(); const two = secret();
  assert.match(one, /^[A-Za-z0-9_-]{43}$/);
  assert.notEqual(one, two);
  assert.notEqual(hash(one), one);
  assert.equal(validPairCode('ABCD2345'), true);
  assert.equal(validPairCode('ABCD01IO'), false);
  assert.equal(validPairCode('ABCD23456'), false);
});
