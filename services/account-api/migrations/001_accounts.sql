BEGIN;
CREATE SCHEMA IF NOT EXISTS kitlane;
CREATE TABLE IF NOT EXISTS kitlane.accounts (
  account_id uuid PRIMARY KEY REFERENCES neon_auth."user"(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS kitlane.devices (
  account_id uuid NOT NULL REFERENCES kitlane.accounts ON DELETE CASCADE,
  device_id uuid NOT NULL,
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 128),
  platform text NOT NULL CHECK (platform IN ('windows','macos','linux')),
  architecture text NOT NULL CHECK (length(architecture) BETWEEN 1 AND 32),
  PRIMARY KEY (account_id, device_id)
);
CREATE TABLE IF NOT EXISTS kitlane.sessions (
  session_id uuid PRIMARY KEY,
  account_id uuid NOT NULL,
  device_id uuid NOT NULL,
  access_hash text UNIQUE NOT NULL,
  refresh_hash text UNIQUE NOT NULL,
  access_expires_at timestamptz NOT NULL,
  refresh_expires_at timestamptz NOT NULL,
  revoked boolean NOT NULL DEFAULT false,
  FOREIGN KEY (account_id, device_id) REFERENCES kitlane.devices ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS kitlane.handoffs (
  attempt_hash text PRIMARY KEY,
  state text NOT NULL,
  challenge text NOT NULL,
  callback text NOT NULL,
  device jsonb NOT NULL,
  expires_at timestamptz NOT NULL,
  code_hash text UNIQUE,
  account_id uuid REFERENCES kitlane.accounts ON DELETE CASCADE,
  completed boolean NOT NULL DEFAULT false,
  consumed boolean NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS handoff_expiry ON kitlane.handoffs(expires_at);
CREATE TABLE IF NOT EXISTS kitlane.pairing_codes (
  code_hash text PRIMARY KEY,
  account_id uuid NOT NULL,
  device_id uuid NOT NULL,
  expires_at timestamptz NOT NULL,
  consumed boolean NOT NULL DEFAULT false,
  FOREIGN KEY (account_id, device_id) REFERENCES kitlane.devices ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS pairing_expiry ON kitlane.pairing_codes(expires_at);
CREATE TABLE IF NOT EXISTS kitlane.pairs (
  account_id uuid NOT NULL,
  device_a uuid NOT NULL,
  device_b uuid NOT NULL,
  CHECK (device_a < device_b),
  PRIMARY KEY (account_id, device_a, device_b),
  FOREIGN KEY (account_id, device_a) REFERENCES kitlane.devices(account_id, device_id) ON DELETE CASCADE,
  FOREIGN KEY (account_id, device_b) REFERENCES kitlane.devices(account_id, device_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS kitlane.snapshots (
  account_id uuid NOT NULL,
  device_id uuid NOT NULL,
  snapshot jsonb NOT NULL,
  PRIMARY KEY (account_id, device_id),
  FOREIGN KEY (account_id, device_id) REFERENCES kitlane.devices ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS kitlane.rate_limits (
  key_hash text PRIMARY KEY,
  starts_at timestamptz NOT NULL,
  attempts integer NOT NULL CHECK (attempts > 0)
);
CREATE INDEX IF NOT EXISTS rate_expiry ON kitlane.rate_limits(starts_at);
REVOKE ALL ON SCHEMA kitlane FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA kitlane FROM PUBLIC;
COMMIT;
