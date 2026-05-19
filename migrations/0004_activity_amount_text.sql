-- Activity finance amounts mirror fin_txn.amount. Store them as canonical
-- signed integer text so cross-module feeds can carry crypto-scale values.
CREATE TABLE activity_new (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at   INTEGER NOT NULL,
    module        TEXT NOT NULL,
    doc_id        TEXT NOT NULL,
    summary       TEXT NOT NULL,
    amount        TEXT,
    status        TEXT,
    link_doc      TEXT,
    currency_code TEXT
);

INSERT INTO activity_new
    (id, occurred_at, module, doc_id, summary, amount, status, link_doc, currency_code)
SELECT
    id,
    occurred_at,
    module,
    doc_id,
    summary,
    CASE WHEN amount IS NULL THEN NULL ELSE CAST(amount AS TEXT) END,
    status,
    link_doc,
    currency_code
FROM activity;

DROP TABLE activity;
ALTER TABLE activity_new RENAME TO activity;

CREATE INDEX activity_occurred ON activity(occurred_at DESC);
CREATE INDEX activity_module   ON activity(module, occurred_at DESC);
CREATE INDEX activity_module_doc ON activity(module, doc_id);
CREATE INDEX activity_link_doc ON activity(link_doc) WHERE link_doc IS NOT NULL;
