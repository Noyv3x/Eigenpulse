CREATE TABLE fit_workout (
    doc_id      TEXT PRIMARY KEY,
    occurred_at INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    program     TEXT,
    duration_m  INTEGER NOT NULL,
    load_text   TEXT,
    strain      TEXT,
    rpe         INTEGER,
    notes       TEXT
);
CREATE INDEX fit_workout_occurred ON fit_workout(occurred_at DESC);

CREATE TABLE fit_set (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    workout_doc  TEXT NOT NULL REFERENCES fit_workout(doc_id) ON DELETE CASCADE,
    exercise     TEXT NOT NULL,
    set_idx      INTEGER NOT NULL,
    reps         INTEGER NOT NULL,
    weight_kg    REAL NOT NULL,
    done         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX fit_set_workout ON fit_set(workout_doc, set_idx);

INSERT INTO seq (module, kind, last_value) VALUES ('FIT', 'type:S', 412);
