-- ⚠ This file IS an applied migration. DO NOT edit it once any database has
-- run it successfully — `sqlx::migrate!()` records its checksum and will refuse
-- to start with `MigrateError::VersionMismatch` if the bytes change. To fix
-- a schema bug, add a new `0002_*.sql` migration instead.
--
-- The single exception in this repo's history: an earlier draft of this file
-- contained `PRAGMA foreign_keys/journal_mode/synchronous` lines, which sqlx
-- rejects because each migration runs inside a transaction. That version
-- never applied anywhere, so removing those PRAGMAs (and moving them to
-- `crates/db/src/pool.rs::open_pool` via `SqliteConnectOptions`) was safe.
-- Do not treat this as license to amend `0001_init.sql` further.

-- Single-row user table.
CREATE TABLE app_user (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    handle        TEXT    NOT NULL DEFAULT 'leo',
    name          TEXT    NOT NULL DEFAULT 'Leo Chen',
    role          TEXT    NOT NULL DEFAULT 'OWNER',
    password_hash TEXT    NOT NULL,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Cookie sessions.
CREATE TABLE session (
    token       TEXT PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES app_user(id),
    issued_at   INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    last_seen   INTEGER NOT NULL
);
CREATE INDEX session_expires ON session(expires_at);

-- Doc-id sequence per (module, kind).
CREATE TABLE seq (
    module      TEXT NOT NULL,
    kind        TEXT NOT NULL,
    last_value  INTEGER NOT NULL,
    PRIMARY KEY (module, kind)
);

-- Module migration ledger (independent of sqlx's _sqlx_migrations).
CREATE TABLE _ep_module_migration (
    module     TEXT NOT NULL,
    name       TEXT NOT NULL,
    applied_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (module, name)
);

-- Cross-module reference graph.
CREATE TABLE module_link (
    source_doc TEXT NOT NULL,
    target_doc TEXT NOT NULL,
    kind       TEXT NOT NULL DEFAULT 'ref',
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (source_doc, target_doc, kind)
);
CREATE INDEX module_link_target ON module_link(target_doc);

-- Activity journal — every module appends a row when an event happens.
CREATE TABLE activity (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at INTEGER NOT NULL,
    module      TEXT NOT NULL,
    doc_id      TEXT NOT NULL,
    summary     TEXT NOT NULL,
    amount      REAL,
    status      TEXT,
    link_doc    TEXT
);
CREATE INDEX activity_occurred ON activity(occurred_at DESC);
CREATE INDEX activity_module   ON activity(module, occurred_at DESC);

-- Notifications (in-app + delivery log).
CREATE TABLE notification (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    severity   TEXT NOT NULL,
    module     TEXT,
    title      TEXT NOT NULL,
    body       TEXT,
    link       TEXT,
    doc_ref    TEXT,
    read_at    INTEGER
);
CREATE INDEX notification_unread ON notification(created_at DESC) WHERE read_at IS NULL;
CREATE INDEX notification_recent ON notification(created_at DESC);

CREATE TABLE notify_channel (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    kind         TEXT NOT NULL,
    name         TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    config_json  TEXT NOT NULL,
    min_severity TEXT NOT NULL DEFAULT 'info',
    created_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX notify_channel_kind ON notify_channel(kind, enabled);

CREATE TABLE notify_delivery (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    notification_id INTEGER NOT NULL REFERENCES notification(id) ON DELETE CASCADE,
    channel_id      INTEGER NOT NULL REFERENCES notify_channel(id) ON DELETE CASCADE,
    attempted_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    ok              INTEGER NOT NULL,
    error           TEXT
);

-- Personal Access Tokens (for /api/v1/* open API).
CREATE TABLE pat (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    prefix       TEXT NOT NULL,
    hash         TEXT NOT NULL,
    scopes       TEXT NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at   INTEGER,
    last_used_at INTEGER,
    revoked_at   INTEGER
);
CREATE INDEX pat_active ON pat(revoked_at) WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX pat_hash ON pat(hash);
