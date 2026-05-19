-- Optional emoji/icon text for user-facing finance categories.
ALTER TABLE fin_category ADD COLUMN icon TEXT NOT NULL DEFAULT '';
