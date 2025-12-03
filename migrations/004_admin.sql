DROP INDEX IF EXISTS idx_addresses_last_selected;

CREATE TABLE IF NOT EXISTS admins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(50) NOT NULL UNIQUE,
    password VARCHAR(255) NOT NULL, 
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_admins_username ON admins (username);

-- Drop the trigger if it already exists before creating it to ensure idempotency.
DROP TRIGGER IF EXISTS set_timestamp ON admins;

-- Create a trigger on the 'admins' table to call the function when a row is updated.
CREATE TRIGGER set_timestamp
BEFORE UPDATE ON admins
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();