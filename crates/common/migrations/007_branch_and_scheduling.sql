-- Multi-branch evaluation and scheduling shares
ALTER TABLE jobsets ADD COLUMN IF NOT EXISTS branch VARCHAR(255) DEFAULT NULL;
ALTER TABLE jobsets ADD COLUMN IF NOT EXISTS scheduling_shares INTEGER NOT NULL DEFAULT 100;
