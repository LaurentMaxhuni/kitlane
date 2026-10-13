import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPair, exportJWK, createLocalJWKSet, SignJWT } from 'jose';
import { verifyIdentity, googleOnly } from '../src/auth.js';

test('accepts signed identity with exact issuer/audience; rejects altered, expired and foreign tokens', async () => {
  const { privateKey, publicKey } = await generateKeyPair('ES256');
  const jwk = { ...await exportJWK(publicKey), kid: 'known', alg: 'ES256' };
  const keys = createLocalJWKSet({ keys: [jwk] });
  const config = { issuer: 'https://auth.example', audience: 'kitlane' };
  const mint = (iss = config.issuer, aud = config.audience, expiry = '1m', kid = 'known') =>
    new SignJWT({}).setProtectedHeader({ alg: 'ES256', kid }).setSubject('11111111-1111-4111-8111-111111111111').setIssuer(iss).setAudience(aud).setIssuedAt().setExpirationTime(expiry).sign(privateKey);
  const token = await mint();
  assert.equal(await verifyIdentity(token, keys, config), '11111111-1111-4111-8111-111111111111');
  for (const bad of [await mint('https://foreign.example'), await mint(config.issuer, 'foreign'), await mint(config.issuer, config.audience, '-1m'), await mint(config.issuer, config.audience, '1m', 'unknown'), token.slice(0, -10) + 'changed']) {
    await assert.rejects(verifyIdentity(bad, keys, config));
  }
});

test('Google identity policy rejects password, linked providers and ambiguous metadata', () => {
  assert.equal(googleOnly([{ providerId: 'google', password: null }]), true);
  for (const accounts of [[], [{ providerId: 'credential', password: 'hash' }], [{ providerId: 'google', password: null }, { providerId: 'github', password: null }], [{ providerId: 'google', password: 'hash' }]]) {
    assert.equal(googleOnly(accounts), false);
  }
});
