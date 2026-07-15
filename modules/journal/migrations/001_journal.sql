-- Journal owns every business record below. It intentionally has no foreign
-- keys to platform tables or other modules.

CREATE TABLE jrn_entry (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title        TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
    body         TEXT NOT NULL DEFAULT '' CHECK (length(body) <= 100000),
    entry_date   TEXT NOT NULL CHECK (
                     length(entry_date) = 10
                     AND entry_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
                 ),
    mood         TEXT CHECK (mood IS NULL OR length(mood) <= 40),
    tags         TEXT NOT NULL DEFAULT '' CHECK (length(tags) <= 1000),
    archived_at  INTEGER,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX jrn_entry_active_date
    ON jrn_entry(archived_at, entry_date DESC, id DESC);
CREATE INDEX jrn_entry_updated
    ON jrn_entry(updated_at DESC, id DESC);

