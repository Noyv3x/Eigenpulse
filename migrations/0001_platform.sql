-- Eigenpulse schema generation 2: shared platform baseline.
--
-- Business data belongs exclusively to module-owned migrations. This global
-- schema intentionally contains no document sequence, cross-module link,
-- activity feed, or module business tables.

CREATE TABLE ep_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO ep_meta (key, value) VALUES ('schema_generation', '2');

-- Single-owner account. The row is created by the first-boot bootstrap.
-- Real instants remain UTC Unix seconds. The persisted IANA timezone only
-- controls presentation and calendar boundaries; browsers update it in
-- automatic mode, while manual mode pins the owner's explicit choice.
CREATE TABLE app_user (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    handle        TEXT    NOT NULL DEFAULT 'owner',
    name          TEXT    NOT NULL DEFAULT 'Owner',
    role          TEXT    NOT NULL DEFAULT 'OWNER',
    password_hash TEXT    NOT NULL,
    locale        TEXT    NOT NULL DEFAULT '',
    timezone      TEXT    NOT NULL DEFAULT 'UTC' CHECK (
        length(timezone) BETWEEN 1 AND 64
        AND timezone = trim(timezone)
    ),
    timezone_mode TEXT    NOT NULL DEFAULT 'auto'
                         CHECK (timezone_mode IN ('auto', 'manual')),
    created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE session (
    token      TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,
    issued_at  INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL
);
CREATE INDEX session_expires ON session(expires_at);

-- Module migrations remain independent from sqlx's platform ledger. The
-- checksum is mandatory from the first install so filename reuse cannot
-- silently apply different SQL to different databases.
CREATE TABLE _ep_module_migration (
    module     TEXT NOT NULL,
    name       TEXT NOT NULL,
    checksum   TEXT NOT NULL,
    applied_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (module, name)
);

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
CREATE UNIQUE INDEX pat_hash ON pat(hash);
CREATE INDEX pat_active ON pat(revoked_at) WHERE revoked_at IS NULL;

-- Notifications are platform messages. `source` identifies the producer but
-- is not a business-table reference and carries no module record id.
CREATE TABLE notification (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    severity   TEXT NOT NULL,
    source     TEXT NOT NULL DEFAULT 'system',
    title      TEXT NOT NULL,
    body       TEXT,
    link       TEXT,
    read_at    INTEGER
);
CREATE INDEX notification_unread ON notification(created_at DESC) WHERE read_at IS NULL;
CREATE INDEX notification_recent ON notification(created_at DESC);
CREATE INDEX notification_source ON notification(source, created_at DESC);

CREATE TABLE notify_channel (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    kind         TEXT NOT NULL,
    name         TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    config_json  TEXT NOT NULL,
    min_severity TEXT NOT NULL DEFAULT 'info',
    created_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX notify_channel_enabled ON notify_channel(id) WHERE enabled = 1;

CREATE TABLE notify_delivery (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    notification_id INTEGER NOT NULL REFERENCES notification(id) ON DELETE CASCADE,
    channel_id      INTEGER NOT NULL REFERENCES notify_channel(id) ON DELETE CASCADE,
    attempted_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    ok              INTEGER NOT NULL CHECK (ok IN (0, 1)),
    error           TEXT
);
CREATE INDEX notify_delivery_notification ON notify_delivery(notification_id);
CREATE INDEX notify_delivery_channel ON notify_delivery(channel_id);

CREATE TABLE notify_outbox (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    notification_id INTEGER NOT NULL REFERENCES notification(id) ON DELETE CASCADE,
    channel_id      INTEGER NOT NULL REFERENCES notify_channel(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending','running','sent','failed','skipped')),
    attempt_count   INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL DEFAULT (unixepoch()),
    lease_until     INTEGER,
    last_error      TEXT,
    created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(notification_id, channel_id)
);
CREATE INDEX notify_outbox_due
    ON notify_outbox(next_attempt_at, id)
    WHERE status = 'pending';
CREATE INDEX notify_outbox_lease
    ON notify_outbox(lease_until, id)
    WHERE status = 'running';
