-- Eigenpulse Finance baseline.
--
-- This schema is intentionally independent from every other business module:
-- all foreign keys stay inside the `fin_*` namespace, every public resource
-- owns an INTEGER id, and transfers are grouped by the module-owned
-- `fin_transfer` aggregate.

CREATE TABLE fin_currency (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    code       TEXT NOT NULL UNIQUE,
    symbol     TEXT NOT NULL,
    remark     TEXT NOT NULL DEFAULT '',
    decimals   INTEGER NOT NULL DEFAULT 2 CHECK (decimals BETWEEN 0 AND 18),
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE UNIQUE INDEX fin_currency_one_primary
    ON fin_currency(is_primary) WHERE is_primary = 1;

INSERT INTO fin_currency (code, symbol, remark, decimals, is_primary, sort_order)
VALUES ('CNY', '¥', '人民币', 2, 1, 0);

CREATE TABLE fin_account (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    currency_id INTEGER NOT NULL REFERENCES fin_currency(id) ON DELETE RESTRICT,
    name        TEXT NOT NULL,
    type        TEXT NOT NULL,
    tone        TEXT NOT NULL DEFAULT '',
    balance     TEXT NOT NULL DEFAULT '0',
    archived    INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (currency_id, name)
);

CREATE INDEX fin_account_currency
    ON fin_account(currency_id, archived, created_at DESC);

CREATE TABLE fin_category (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    currency_id INTEGER NOT NULL REFERENCES fin_currency(id) ON DELETE RESTRICT,
    name        TEXT NOT NULL,
    icon        TEXT NOT NULL DEFAULT '',
    tone        TEXT NOT NULL DEFAULT '',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    archived    INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (currency_id, name)
);

CREATE INDEX fin_category_currency
    ON fin_category(currency_id, archived, sort_order, id);

CREATE TABLE fin_transfer (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at     INTEGER NOT NULL,
    occurred_on     TEXT NOT NULL CHECK (
        length(occurred_on) = 10
        AND occurred_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    from_account_id INTEGER NOT NULL REFERENCES fin_account(id) ON DELETE RESTRICT,
    to_account_id   INTEGER NOT NULL REFERENCES fin_account(id) ON DELETE RESTRICT,
    from_amount     TEXT NOT NULL,
    to_amount       TEXT NOT NULL,
    note            TEXT,
    created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK (from_account_id <> to_account_id)
);

CREATE INDEX fin_transfer_occurred
    ON fin_transfer(occurred_at DESC, id DESC);
CREATE INDEX fin_transfer_business_date
    ON fin_transfer(occurred_on DESC, id DESC);

CREATE TABLE fin_txn (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    currency_id   INTEGER NOT NULL REFERENCES fin_currency(id) ON DELETE RESTRICT,
    occurred_at   INTEGER NOT NULL,
    occurred_on   TEXT NOT NULL CHECK (
        length(occurred_on) = 10
        AND occurred_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    merchant      TEXT NOT NULL,
    category_id   INTEGER REFERENCES fin_category(id) ON DELETE RESTRICT,
    account_id    INTEGER NOT NULL REFERENCES fin_account(id) ON DELETE RESTRICT,
    amount        TEXT NOT NULL,
    tag           TEXT NOT NULL CHECK (tag IN ('exp', 'inc', 'tfr')),
    note          TEXT,
    transfer_id   INTEGER REFERENCES fin_transfer(id) ON DELETE CASCADE,
    transfer_role TEXT CHECK (transfer_role IN ('out', 'in')),
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK (
        (tag = 'tfr' AND category_id IS NULL AND transfer_id IS NOT NULL AND transfer_role IS NOT NULL)
        OR
        (tag IN ('exp', 'inc') AND category_id IS NOT NULL AND transfer_id IS NULL AND transfer_role IS NULL)
    )
);

CREATE UNIQUE INDEX fin_txn_transfer_role
    ON fin_txn(transfer_id, transfer_role) WHERE transfer_id IS NOT NULL;
CREATE INDEX fin_txn_page
    ON fin_txn(currency_id, occurred_at DESC, id DESC);
CREATE INDEX fin_txn_account
    ON fin_txn(account_id, occurred_at DESC, id DESC);
CREATE INDEX fin_txn_category
    ON fin_txn(category_id, occurred_at DESC, id DESC)
    WHERE category_id IS NOT NULL;
CREATE INDEX fin_txn_business_period
    ON fin_txn(currency_id, occurred_on, tag);
CREATE INDEX fin_txn_business_category
    ON fin_txn(currency_id, category_id, occurred_on)
    WHERE tag = 'exp';

CREATE TABLE fin_budget (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    currency_id INTEGER NOT NULL REFERENCES fin_currency(id) ON DELETE CASCADE,
    period      TEXT NOT NULL,
    category_id INTEGER NOT NULL REFERENCES fin_category(id) ON DELETE CASCADE,
    amount      TEXT NOT NULL,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (currency_id, period, category_id)
);

CREATE INDEX fin_budget_period
    ON fin_budget(currency_id, period, category_id);
