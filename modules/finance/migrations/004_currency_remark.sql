-- Currency codes are the primary identifier. Keep the auxiliary user text,
-- but expose it as a remark instead of a name.
ALTER TABLE fin_currency RENAME COLUMN name TO remark;
