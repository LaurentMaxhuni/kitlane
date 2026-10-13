import { defineConfig } from '@neon/config/v1';

export default defineConfig({
  functions: {
    kitlane: {
      name: 'Kitlane accounts',
      source: 'dist/index.mjs',
      bundler: 'none',
      env: {
        KITLANE_ORIGIN: process.env.KITLANE_ORIGIN!,
        KITLANE_AUTH_ISSUER: process.env.KITLANE_AUTH_ISSUER!,
        KITLANE_AUTH_AUDIENCE: process.env.KITLANE_AUTH_AUDIENCE!,
      },
    },
  },
});
