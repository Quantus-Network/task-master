ALTER TABLE addresses
DROP COLUMN IF EXISTS eth_address;

ALTER TABLE addresses
DROP COLUMN IF EXISTS last_selected_at;

ALTER TABLE addresses
ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

DROP TRIGGER IF EXISTS set_timestamp_addresses ON addresses;

CREATE TRIGGER set_timestamp_addresses
BEFORE UPDATE ON addresses
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();


-- eth_associations table ---------------------------------------------------
CREATE TABLE IF NOT EXISTS eth_associations (
    quan_address VARCHAR(64) PRIMARY KEY
        REFERENCES addresses(quan_address) ON DELETE CASCADE,
    eth_address VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT unique_eth_address UNIQUE (eth_address)
);

-- Index for fast search on eth_address
CREATE INDEX IF NOT EXISTS idx_eth_associations_eth_address
    ON eth_associations (eth_address);


-- x_associations table -----------------------------------------------------
CREATE TABLE IF NOT EXISTS x_associations (
    quan_address VARCHAR(64) PRIMARY KEY
        REFERENCES addresses(quan_address) ON DELETE CASCADE,
    username VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT unique_x_username UNIQUE (username)
);

-- Index for fast search on x username
CREATE INDEX IF NOT EXISTS idx_x_associations_username
    ON x_associations (username);