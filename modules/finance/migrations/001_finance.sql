CREATE TABLE fin_account (
    code     TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    type     TEXT NOT NULL,
    tone     TEXT NOT NULL DEFAULT '',
    balance  REAL NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE fin_category (
    code       TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    tone       TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0
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

CREATE TABLE fin_budget (
    period        TEXT NOT NULL,
    category_code TEXT NOT NULL REFERENCES fin_category(code),
    amount        REAL NOT NULL,
    PRIMARY KEY (period, category_code)
);

INSERT INTO fin_account (code, name, type, tone, balance) VALUES
 ('ACC-01','招商银行 · 主卡','Checking','blue',18421.40),
 ('ACC-02','支付宝余额宝','Savings','green',22800.00),
 ('ACC-03','投资账户 / ETF','Investment','violet',15420.88),
 ('ACC-04','现金 / 备用金','Cash','',1200.00);

INSERT INTO fin_category (code, name, tone, sort_order) VALUES
 ('F&B','餐饮','amber',1),
 ('TRN','交通','blue',2),
 ('HLT','健身','green',3),
 ('EDU','学习','violet',4),
 ('HSE','居家','rose',5),
 ('OTH','其他','',6),
 ('INC','收入','green',7),
 ('TFR','转账','blue',8);

INSERT INTO fin_budget (period, category_code, amount) VALUES
 ('2026-04','F&B',3200),('2026-04','TRN',1600),('2026-04','HLT',1200),
 ('2026-04','EDU',1500),('2026-04','HSE',2000),('2026-04','OTH',1500);

INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code, account_code, amount, tag, linked_doc_id) VALUES
 ('FIN-24091', unixepoch('2026-04-22 09:02:00'), 'Blue Bottle · 上海',  'F&B', 'ACC-01',  -42.00, 'exp', NULL),
 ('FIN-24090', unixepoch('2026-04-21 20:41:00'), 'Keep Gym · 月卡续费',  'HLT', 'ACC-01', -298.00, 'exp', NULL),
 ('FIN-24089', unixepoch('2026-04-21 12:15:00'), '工资 · 入账',          'INC', 'ACC-01', 18400.00, 'inc', NULL),
 ('FIN-24088', unixepoch('2026-04-20 19:22:00'), 'Kindle · 《深度工作》','EDU', 'ACC-01',  -38.00, 'exp', NULL),
 ('FIN-24087', unixepoch('2026-04-20 13:05:00'), '盒马 · 生鲜',          'HSE', 'ACC-01', -186.50, 'exp', NULL),
 ('FIN-24086', unixepoch('2026-04-19 15:44:00'), '转账 · 余额宝',        'TFR', 'ACC-02', 3000.00, 'tfr', NULL),
 ('FIN-24085', unixepoch('2026-04-19 11:12:00'), '滴滴出行',             'TRN', 'ACC-01',  -24.80, 'exp', NULL),
 ('FIN-24084', unixepoch('2026-04-18 21:10:00'), '蛋白粉 · 海淘',        'HLT', 'ACC-01', -428.00, 'exp', NULL);

INSERT INTO seq (module, kind, last_value) VALUES ('FIN', 'doc:y24', 91);

INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc) VALUES
 (unixepoch('2026-04-22 09:02:00'), 'FIN', 'FIN-24091', 'Blue Bottle · 上海',           -42.00, NULL),
 (unixepoch('2026-04-21 20:41:00'), 'FIN', 'FIN-24090', 'Keep Gym · 月卡续费',         -298.00, NULL),
 (unixepoch('2026-04-21 12:15:00'), 'FIN', 'FIN-24089', '工资 · 入账',                18400.00, NULL),
 (unixepoch('2026-04-20 19:22:00'), 'FIN', 'FIN-24088', 'Kindle · 《深度工作》',        -38.00, NULL),
 (unixepoch('2026-04-20 13:05:00'), 'FIN', 'FIN-24087', '盒马 · 生鲜',                 -186.50, NULL);
