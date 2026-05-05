-- created_at uses literal `DEFAULT 0` + UPDATE back-fill: SQLite forbids
-- expression DEFAULTs on ALTER ADD COLUMN. See CLAUDE.md migration discipline.
ALTER TABLE fin_category ADD COLUMN archived   INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fin_category ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fin_account  ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;

UPDATE fin_category SET created_at = unixepoch() WHERE created_at = 0;
UPDATE fin_account  SET created_at = unixepoch() WHERE created_at = 0;

CREATE UNIQUE INDEX fin_txn_transfer_pair
    ON fin_txn(linked_doc_id)
    WHERE tag='tfr' AND linked_doc_id IS NOT NULL;
