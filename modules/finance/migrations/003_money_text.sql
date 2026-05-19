-- Crypto-ready money storage: keep amounts as canonical signed integer text.
--
-- INTEGER minor units work for fiat and BTC-style 8 decimals, but 18-decimal
-- crypto assets overflow i64 at everyday balances. TEXT avoids SQLite's
-- integer limit; application code decodes into MinorAmount(i128) and performs
-- exact aggregation in Rust.

ALTER TABLE fin_account RENAME TO fin_account_old;
ALTER TABLE fin_txn     RENAME TO fin_txn_old;
ALTER TABLE fin_budget  RENAME TO fin_budget_old;

CREATE TABLE fin_account (
    currency_code TEXT NOT NULL REFERENCES fin_currency(code),
    code          TEXT NOT NULL,
    name          TEXT NOT NULL,
    type          TEXT NOT NULL,
    tone          TEXT NOT NULL DEFAULT '',
    balance       TEXT NOT NULL DEFAULT '0',
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
    amount        TEXT NOT NULL,
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
    amount        TEXT NOT NULL,
    PRIMARY KEY (currency_code, period, category_code),
    FOREIGN KEY (currency_code, category_code) REFERENCES fin_category(currency_code, code)
);

INSERT INTO fin_account (currency_code, code, name, type, tone, balance, archived, created_at)
SELECT currency_code, code, name, type, tone, CAST(balance AS TEXT), archived, created_at
  FROM fin_account_old;

INSERT INTO fin_txn
    (doc_id, currency_code, occurred_at, merchant, category_code, account_code,
     amount, tag, note, linked_doc_id, created_at)
SELECT doc_id, currency_code, occurred_at, merchant, category_code, account_code,
       CAST(amount AS TEXT), tag, note, linked_doc_id, created_at
  FROM fin_txn_old;

INSERT INTO fin_budget (currency_code, period, category_code, amount)
SELECT currency_code, period, category_code, CAST(amount AS TEXT)
  FROM fin_budget_old;

DROP TABLE fin_txn_old;
DROP TABLE fin_budget_old;
DROP TABLE fin_account_old;

CREATE INDEX fin_txn_occurred ON fin_txn(currency_code, occurred_at DESC);
CREATE INDEX fin_txn_account  ON fin_txn(currency_code, account_code, occurred_at DESC);
CREATE INDEX fin_txn_category ON fin_txn(currency_code, category_code, occurred_at DESC);
CREATE INDEX fin_txn_link     ON fin_txn(linked_doc_id) WHERE linked_doc_id IS NOT NULL;
CREATE UNIQUE INDEX fin_txn_transfer_pair
    ON fin_txn(linked_doc_id)
    WHERE tag = 'tfr' AND linked_doc_id IS NOT NULL;
