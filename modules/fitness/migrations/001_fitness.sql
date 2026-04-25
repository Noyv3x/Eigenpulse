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

INSERT INTO fit_workout (doc_id, occurred_at, kind, program, duration_m, load_text, strain) VALUES
 ('FIT-S-0412', unixepoch('2026-04-22 18:30:00'), '力量 · 推日',  'PPL-5D', 62, '7,840kg',  'M'),
 ('FIT-S-0411', unixepoch('2026-04-21 07:30:00'), '有氧 · Z2',    'Base',   45, '6.2km',    'L'),
 ('FIT-S-0410', unixepoch('2026-04-19 18:30:00'), '力量 · 拉日',  'PPL-5D', 58, '6,210kg',  'M'),
 ('FIT-S-0409', unixepoch('2026-04-18 18:30:00'), '力量 · 腿日',  'PPL-5D', 74, '12,400kg', 'H'),
 ('FIT-S-0408', unixepoch('2026-04-17 07:30:00'), '有氧 · HIIT',  'Base',   28, '3.1km',    'H');

INSERT INTO seq (module, kind, last_value) VALUES ('FIT', 'type:S', 412);
