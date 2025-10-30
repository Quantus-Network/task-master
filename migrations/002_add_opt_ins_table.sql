CREATE TABLE IF NOT EXISTS opt_ins (
    quan_address VARCHAR(64) PRIMARY KEY REFERENCES addresses(quan_address) ON DELETE CASCADE,
    opted_in_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    opt_in_number INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_opt_ins_opted_in_at ON opt_ins (opted_in_at);
CREATE INDEX IF NOT EXISTS idx_opt_ins_opt_in_number ON opt_ins (opt_in_number);
CREATE INDEX IF NOT EXISTS idx_opt_ins_composite ON opt_ins (opted_in_at) 
WHERE opted_in_at IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_opt_ins_number_unique ON opt_ins (opt_in_number);

ALTER TABLE addresses DROP COLUMN IF EXISTS is_reward_program_participant;

