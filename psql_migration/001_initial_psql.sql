-- Create a reusable function to automatically update the 'updated_at' timestamp on row changes.
CREATE OR REPLACE FUNCTION trigger_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Addresses table
CREATE TABLE addresses (
    quan_address VARCHAR(64) PRIMARY KEY,
    eth_address VARCHAR(64),
    referral_code VARCHAR(7) UNIQUE,
    referrals_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_selected_at TIMESTAMPTZ
);

CREATE INDEX idx_addresses_last_selected ON addresses (last_selected_at);

-- Referrals table
CREATE TABLE referrals (
    id SERIAL PRIMARY KEY,
    -- The user who owns this referral code
    referrer_address VARCHAR(64) NOT NULL REFERENCES addresses(quan_address) ON DELETE CASCADE,
    -- The user who signed up using this referral code
    referee_address VARCHAR(64) UNIQUE REFERENCES addresses(quan_address) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_referrals_referrer ON referrals(referrer_address);

-- Tasks table
CREATE TABLE tasks (
    id SERIAL PRIMARY KEY,
    task_id TEXT UNIQUE NOT NULL,
    quan_address VARCHAR(64) NOT NULL REFERENCES addresses (quan_address),
    quan_amount BIGINT NOT NULL, -- Changed to BIGINT for larger integer values
    usdc_amount BIGINT NOT NULL, -- Changed to BIGINT for larger integer values
    task_url TEXT UNIQUE NOT NULL,
    status VARCHAR(64) NOT NULL DEFAULT 'pending',
    reversible_tx_id TEXT,
    send_time TIMESTAMPTZ,
    end_time TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create a trigger on the 'tasks' table to call the function when a row is updated.
CREATE TRIGGER set_timestamp
BEFORE UPDATE ON tasks
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();

-- Indexes for the tasks table
CREATE INDEX idx_tasks_status ON tasks (status);
CREATE INDEX idx_tasks_end_time ON tasks (end_time);
CREATE INDEX idx_tasks_task_url ON tasks (task_url);
CREATE INDEX idx_tasks_quan_address ON tasks (quan_address);