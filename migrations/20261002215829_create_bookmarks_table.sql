-- Up migration
CREATE TABLE bookmarks (
    id UUID PRIMARY KEY,
    url TEXT NOT NULL,
    title TEXT,
    raw_html TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- If you have an old items table you want to get rid of, you can drop it here:
DROP TABLE IF EXISTS items;
