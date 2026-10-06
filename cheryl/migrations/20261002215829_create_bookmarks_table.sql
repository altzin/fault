CREATE TABLE IF NOT EXISTS bookmarks (
    id UUID PRIMARY KEY,
    url TEXT NOT NULL,
    title TEXT,
    status TEXT NOT NULL 
        CHECK (status IN ('pending', 'processing', 'completed', 'failed')) 
        DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bookmarks_created_at ON bookmarks (created_at DESC);

CREATE OR REPLACE FUNCTION trigger_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_timestamp
BEFORE UPDATE ON bookmarks
FOR EACH ROW
EXECUTE FUNCTION trigger_set_timestamp();
