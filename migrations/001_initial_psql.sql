-- Create a reusable function to automatically update the 'updated_at' timestamp on row changes.
-- This is already idempotent due to CREATE OR REPLACE.
CREATE OR REPLACE FUNCTION trigger_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Addresses table
CREATE TABLE IF NOT EXISTS addresses (
    quan_address VARCHAR(64) PRIMARY KEY,
    eth_address VARCHAR(64),
    referral_code VARCHAR(255) UNIQUE,
    referrals_count INTEGER DEFAULT 0,
    is_reward_program_participant BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_selected_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_addresses_last_selected ON addresses (last_selected_at);
CREATE INDEX IF NOT EXISTS idx_addresses_referral_code ON addresses (referral_code);

-- Referrals table
CREATE TABLE IF NOT EXISTS referrals (
    id SERIAL PRIMARY KEY,
    referrer_address VARCHAR(64) NOT NULL REFERENCES addresses(quan_address),
    referee_address VARCHAR(64) NOT NULL REFERENCES addresses(quan_address),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ensures that the combination of a referrer and a referee is unique.
    CONSTRAINT unique_referral_pair UNIQUE (referrer_address, referee_address)
);

CREATE INDEX IF NOT EXISTS idx_referrals_referrer ON referrals(referrer_address);

---

-- Tasks table
CREATE TABLE IF NOT EXISTS tasks (
    id SERIAL PRIMARY KEY,
    task_id TEXT UNIQUE NOT NULL,
    quan_address VARCHAR(64) NOT NULL REFERENCES addresses (quan_address),
    quan_amount BIGINT NOT NULL,
    usdc_amount BIGINT NOT NULL,
    task_url TEXT UNIQUE NOT NULL,
    status VARCHAR(64) NOT NULL DEFAULT 'pending',
    reversible_tx_id TEXT,
    send_time TIMESTAMPTZ,
    end_time TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Drop the trigger if it already exists before creating it to ensure idempotency.
DROP TRIGGER IF EXISTS set_timestamp ON tasks;

-- Create a trigger on the 'tasks' table to call the function when a row is updated.
CREATE TRIGGER set_timestamp
BEFORE UPDATE ON tasks
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();

-- Indexes for the tasks table
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks (status);
CREATE INDEX IF NOT EXISTS idx_tasks_end_time ON tasks (end_time);
CREATE INDEX IF NOT EXISTS idx_tasks_task_url ON tasks (task_url);
CREATE INDEX IF NOT EXISTS idx_tasks_quan_address ON tasks (quan_address);