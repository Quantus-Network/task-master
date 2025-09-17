-- Initial migration for TaskMaster database
-- Creates addresses and tasks tables

-- Addresses table (candidates)
CREATE TABLE addresses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    quan_address TEXT UNIQUE NOT NULL,
    eth_address TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_selected_at TIMESTAMP
);

-- Tasks table
CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT UNIQUE NOT NULL,
    quan_address TEXT NOT NULL,
    quan_amount INTEGER NOT NULL,
    usdc_amount INTEGER NOT NULL,
    task_url TEXT UNIQUE NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    reversible_tx_id TEXT,
    send_time TIMESTAMP,
    end_time TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (quan_address) REFERENCES addresses (quan_address)
);

-- Indexes for performance
CREATE INDEX idx_tasks_status ON tasks (status);
CREATE INDEX idx_tasks_end_time ON tasks (end_time);
CREATE INDEX idx_tasks_task_url ON tasks (task_url);
CREATE INDEX idx_addresses_last_selected ON addresses (last_selected_at);
CREATE INDEX idx_tasks_quan_address ON tasks (quan_address);
