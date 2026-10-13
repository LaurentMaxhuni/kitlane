-- Existing registrations without installation proof remain unusable until recovered.
DO $migration$
BEGIN
  ALTER TABLE kitlane.devices ADD COLUMN IF NOT EXISTS installation_hash text CHECK (installation_hash ~ '^[0-9a-f]{64}$');
  ALTER TABLE kitlane.handoffs ADD COLUMN IF NOT EXISTS installation_hash text CHECK (installation_hash ~ '^[0-9a-f]{64}$');
END $migration$;
