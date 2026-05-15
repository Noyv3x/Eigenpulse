-- Multi-currency: every account / category / transaction / budget belongs to
-- exactly one currency. Currencies are isolated -- there is no cross-currency
-- conversion. Money moves from REAL (major units) to INTEGER (minor units,
-- scaled by 10^decimals of the owning currency). Pre-existing rows are
-- assigned to the default CNY currency and scaled x100 (the old implicit
-- 2-decimal yuan representation).
--
-- This migration runs inside one transaction with foreign_keys ON, so the
-- table rebuild renames the old tables aside, creates the new shapes, copies
-- parents before children, then drops the old tables children-first.

-- 1. Currency registry. `code` is the immutable identifier; symbol / name /
--    decimals are user-editable. Exactly one row carries is_primary = 1.
CREATE TABLE fin_currency (
    code       TEXT PRIMARY KEY,
    symbol     TEXT NOT NULL,
    name       TEXT NOT NULL,
    decimals   INTEGER NOT NULL DEFAULT 2,
    is_primary INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
INSERT INTO fin_currency (code, symbol, name, decimals, is_primary, sort_order)
VALUES ('CNY', '¥', '人民币', 2, 1, 0);

-- 2. Rename the four money tables aside. SQLite rewrites foreign-key text in
--    dependent tables on RENAME, but every renamed table is dropped below,
--    so the dangling references never matter.
ALTER TABLE fin_account  RENAME TO fin_account_old;
ALTER TABLE fin_category RENAME TO fin_category_old;
ALTER TABLE fin_txn      RENAME TO fin_txn_old;
ALTER TABLE fin_budget   RENAME TO fin_budget_old;

-- 3. Recreate them with their final names: composite (currency_code, code)
--    primary keys, integer minor-unit amounts, currency foreign keys.
CREATE TABLE fin_account (
    currency_code TEXT NOT NULL REFERENCES fin_currency(code),
    code          TEXT NOT NULL,
    name          TEXT NOT NULL,
    type          TEXT NOT NULL,
    tone          TEXT NOT NULL DEFAULT '',
    balance       INTEGER NOT NULL DEFAULT 0,
    archived      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (currency_code, code)
);

CREATE TABLE fin_category (
    currency_code TEXT NOT NULL REFERENCES fin_currency(code),
    code          TEXT NOT NULL,
    name          TEXT NOT NULL,
    tone          TEXT NOT NULL DEFAULT '',
    sort_order    INTEGER NOT NULL DEFAULT 0,
    archived      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (currency_code, code)
);

CREATE TABLE fin_txn (
    doc_id        TEXT PRIMARY KEY,
    currency_code TEXT NOT NULL REFERENCES fin_currency(code),
    occurred_at   INTEGER NOT NULL,
    merchant      TEXT NOT NULL,
    category_code TEXT NOT NULL,
    account_code  TEXT NOT NULL,
    amount        INTEGER NOT NULL,
    tag           TEXT NOT NULL,
    note          TEXT,
    linked_doc_id TEXT,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (currency_code, category_code) REFERENCES fin_category(currency_code, code),
    FOREIGN KEY (currency_code, account_code)  REFERENCES fin_account(currency_code, code)
);

CREATE TABLE fin_budget (
    currency_code TEXT NOT NULL REFERENCES fin_currency(code),
    period        TEXT NOT NULL,
    category_code TEXT NOT NULL,
    amount        INTEGER NOT NULL,
    PRIMARY KEY (currency_code, period, category_code),
    FOREIGN KEY (currency_code, category_code) REFERENCES fin_category(currency_code, code)
);

-- 4. Copy data forward, assigning everything to CNY and scaling money x100.
--    Parents (account, category) before children (txn, budget) so the
--    row-by-row foreign-key checks pass.
INSERT INTO fin_account (currency_code, code, name, type, tone, balance, archived, created_at)
SELECT 'CNY', code, name, type, tone,
       CAST(ROUND(balance * 100) AS INTEGER), archived, created_at
  FROM fin_account_old;

INSERT INTO fin_category (currency_code, code, name, tone, sort_order, archived, created_at)
SELECT 'CNY', code, name, tone, sort_order, archived, created_at
  FROM fin_category_old;

-- Every currency needs a 'TFR' category for transfer legs. 001_finance.sql
-- never seeded it; INSERT OR IGNORE back-fills CNY and is a no-op when a
-- pre-existing database already had a user-made TFR row copied just above.
INSERT OR IGNORE INTO fin_category (currency_code, code, name, tone, sort_order, archived, created_at)
VALUES ('CNY', 'TFR', 'Transfer', '', 999, 0, unixepoch());

INSERT INTO fin_txn (doc_id, currency_code, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id, created_at)
SELECT doc_id, 'CNY', occurred_at, merchant, category_code, account_code,
       CAST(ROUND(amount * 100) AS INTEGER), tag, note, linked_doc_id, created_at
  FROM fin_txn_old;

INSERT INTO fin_budget (currency_code, period, category_code, amount)
SELECT 'CNY', period, category_code, CAST(ROUND(amount * 100) AS INTEGER)
  FROM fin_budget_old;

-- 5. Drop the old tables children-first so the implicit DELETE that DROP
--    performs while foreign keys are enabled never trips a constraint.
DROP TABLE fin_txn_old;
DROP TABLE fin_budget_old;
DROP TABLE fin_account_old;
DROP TABLE fin_category_old;

-- 6. Indexes -- all currency-scoped now, since every view filters by currency.
-- `fin_account` and `fin_budget` get no extra `currency_code` index: their
-- composite primary keys lead with `currency_code`, so SQLite already uses
-- the PK index for any `WHERE currency_code = ?` lookup. `fin_category` does
-- need its own (currency_code, sort_order) index — the PK is keyed on `code`,
-- not `sort_order`, so without this index the ORDER BY sort_order pass would
-- be unsorted.
CREATE INDEX fin_txn_occurred ON fin_txn(currency_code, occurred_at DESC);
CREATE INDEX fin_txn_account  ON fin_txn(currency_code, account_code, occurred_at DESC);
CREATE INDEX fin_txn_category ON fin_txn(currency_code, category_code, occurred_at DESC);
CREATE INDEX fin_txn_link     ON fin_txn(linked_doc_id) WHERE linked_doc_id IS NOT NULL;
CREATE UNIQUE INDEX fin_txn_transfer_pair
    ON fin_txn(linked_doc_id)
    WHERE tag = 'tfr' AND linked_doc_id IS NOT NULL;
CREATE INDEX fin_category_currency ON fin_category(currency_code, sort_order);
