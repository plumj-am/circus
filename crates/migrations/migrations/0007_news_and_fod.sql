-- Announcements/news system
CREATE TABLE news (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    title      TEXT        NOT NULL,
    content    TEXT        NOT NULL,
    created_by UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Fixed-output derivation tracking on builds
ALTER TABLE builds
    ADD COLUMN IF NOT EXISTS is_fod   BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS fod_hash TEXT;

CREATE INDEX IF NOT EXISTS builds_is_fod_idx ON builds (is_fod) WHERE is_fod = TRUE;
