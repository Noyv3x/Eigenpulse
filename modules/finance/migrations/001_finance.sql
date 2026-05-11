CREATE TABLE fin_account (
    code       TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    type       TEXT NOT NULL,
    tone       TEXT NOT NULL DEFAULT '',
    balance    REAL NOT NULL DEFAULT 0,
    archived   INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE fin_category (
    code       TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    tone       TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    archived   INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE fin_txn (
    doc_id        TEXT PRIMARY KEY,
    occurred_at   INTEGER NOT NULL,
    merchant      TEXT NOT NULL,
    category_code TEXT NOT NULL REFERENCES fin_category(code),
    account_code  TEXT NOT NULL REFERENCES fin_account(code),
    amount        REAL NOT NULL,
    tag           TEXT NOT NULL,
    note          TEXT,
    linked_doc_id TEXT,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX fin_txn_occurred ON fin_txn(occurred_at DESC);
CREATE INDEX fin_txn_account  ON fin_txn(account_code, occurred_at DESC);
CREATE INDEX fin_txn_category ON fin_txn(category_code, occurred_at DESC);
CREATE INDEX fin_txn_link     ON fin_txn(linked_doc_id) WHERE linked_doc_id IS NOT NULL;
CREATE UNIQUE INDEX fin_txn_transfer_pair
    ON fin_txn(linked_doc_id)
    WHERE tag='tfr' AND linked_doc_id IS NOT NULL;

CREATE TABLE fin_budget (
    period        TEXT NOT NULL,
    category_code TEXT NOT NULL REFERENCES fin_category(code),
    amount        REAL NOT NULL,
    PRIMARY KEY (period, category_code)
);

INSERT INTO seq (module, kind, last_value) VALUES ('FIN', 'doc:y24', 91);
