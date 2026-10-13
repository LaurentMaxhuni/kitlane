export type Config = { origin: string; authURL: string; jwksURL: string; issuer: string; audience: string };

export function config(env = process.env): Config {
  const required = (key: string) => { const value = env[key]; if (!value) throw new Error(`${key} is required`); return value; };
  const origin = new URL(required('KITLANE_ORIGIN'));
  const auth = new URL(required('NEON_AUTH_BASE_URL'));
  const jwks = new URL(required('NEON_AUTH_JWKS_URL'));
  if (origin.protocol !== 'https:' || origin.href !== origin.origin+'/' || auth.protocol !== 'https:' || jwks.protocol !== 'https:' || jwks.origin !== auth.origin || origin.username || auth.username || jwks.username || origin.password || auth.password || jwks.password) throw new Error('Invalid HTTPS configuration');
  const issuer=required('KITLANE_AUTH_ISSUER');
  if(issuer!==auth.origin&&issuer!==auth.href.replace(/\/$/,''))throw new Error('Issuer must match the configured Auth branch');
  return { origin: origin.origin, authURL: auth.href.replace(/\/$/,''), jwksURL: jwks.href, issuer, audience: required('KITLANE_AUTH_AUDIENCE') };
}
