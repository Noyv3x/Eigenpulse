-- Fitness owns every business table below. All foreign keys remain inside the
-- `fit_*` namespace and each resource uses a module-local integer identity.

CREATE TABLE fit_settings (
    id                            INTEGER PRIMARY KEY CHECK (id = 1),
    unit_system                   TEXT NOT NULL DEFAULT 'metric'
                                    CHECK (unit_system IN ('metric', 'imperial')),
    weekly_workout_target         INTEGER NOT NULL DEFAULT 3
                                    CHECK (weekly_workout_target BETWEEN 1 AND 14),
    weekly_cardio_minutes_target  INTEGER NOT NULL DEFAULT 150
                                    CHECK (weekly_cardio_minutes_target BETWEEN 0 AND 10080),
    updated_at                    INTEGER NOT NULL DEFAULT (unixepoch())
);
INSERT INTO fit_settings (id) VALUES (1);

CREATE TABLE fit_exercise (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL COLLATE NOCASE,
    category        TEXT NOT NULL DEFAULT 'strength'
                        CHECK (category IN ('strength', 'cardio', 'mobility', 'other')),
    tracking_mode   TEXT NOT NULL DEFAULT 'weighted'
                        CHECK (tracking_mode IN
                            ('weighted', 'reps', 'duration', 'distance', 'bodyweight', 'assisted')),
    primary_muscle  TEXT,
    equipment       TEXT,
    notes           TEXT,
    archived        INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (name)
);
CREATE INDEX fit_exercise_active_name ON fit_exercise(archived, name);

-- object_key is an opaque, server-generated filename. File bytes live under
-- /data/modules/fitness/media/objects and are never accepted from this table.
CREATE TABLE fit_exercise_media (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    exercise_id   INTEGER NOT NULL REFERENCES fit_exercise(id) ON DELETE CASCADE,
    object_key    TEXT NOT NULL UNIQUE,
    title         TEXT,
    media_type    TEXT NOT NULL CHECK (media_type IN ('gif', 'mp4', 'webm')),
    byte_size     INTEGER NOT NULL CHECK (byte_size > 0),
    sha256        TEXT NOT NULL CHECK (length(sha256) = 64),
    -- 100..111 is reserved for an in-transaction reorder staging pass; all
    -- committed application rows are normalized back to 0..11.
    sort_order    INTEGER NOT NULL CHECK (sort_order BETWEEN 0 AND 111),
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (exercise_id, sort_order)
);
CREATE INDEX fit_exercise_media_exercise
    ON fit_exercise_media(exercise_id, sort_order);

CREATE TRIGGER fit_exercise_media_limit_insert
BEFORE INSERT ON fit_exercise_media
WHEN (SELECT COUNT(*) FROM fit_exercise_media WHERE exercise_id = NEW.exercise_id) >= 12
BEGIN
    SELECT RAISE(ABORT, 'exercise media limit exceeded');
END;

CREATE TRIGGER fit_exercise_media_limit_move
BEFORE UPDATE OF exercise_id ON fit_exercise_media
WHEN NEW.exercise_id <> OLD.exercise_id
 AND (SELECT COUNT(*) FROM fit_exercise_media WHERE exercise_id = NEW.exercise_id) >= 12
BEGIN
    SELECT RAISE(ABORT, 'exercise media limit exceeded');
END;

CREATE TABLE fit_plan (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    notes       TEXT,
    archived    INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX fit_plan_active_name ON fit_plan(archived, name);

CREATE TABLE fit_plan_exercise (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_id      INTEGER NOT NULL REFERENCES fit_plan(id) ON DELETE CASCADE,
    exercise_id  INTEGER NOT NULL REFERENCES fit_exercise(id) ON DELETE RESTRICT,
    position     INTEGER NOT NULL CHECK (position >= 0),
    notes        TEXT,
    UNIQUE (plan_id, position)
);

CREATE TABLE fit_plan_set (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_exercise_id   INTEGER NOT NULL REFERENCES fit_plan_exercise(id) ON DELETE CASCADE,
    position           INTEGER NOT NULL CHECK (position >= 0),
    target_reps        INTEGER CHECK (target_reps > 0),
    target_weight_g    INTEGER CHECK (target_weight_g >= 0),
    target_duration_s  INTEGER CHECK (target_duration_s > 0),
    target_distance_m  INTEGER CHECK (target_distance_m > 0),
    target_rpe_x10     INTEGER CHECK (target_rpe_x10 BETWEEN 10 AND 100),
    set_type           TEXT NOT NULL DEFAULT 'working'
                            CHECK (set_type IN ('warmup', 'working', 'drop', 'failure')),
    rest_seconds       INTEGER NOT NULL DEFAULT 90 CHECK (rest_seconds BETWEEN 0 AND 3600),
    UNIQUE (plan_exercise_id, position)
);

CREATE TABLE fit_workout (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_id              INTEGER REFERENCES fit_plan(id) ON DELETE SET NULL,
    plan_name_snapshot   TEXT,
    status               TEXT NOT NULL DEFAULT 'in_progress'
                             CHECK (status IN ('in_progress', 'paused', 'completed')),
    workout_date         TEXT NOT NULL CHECK (
        length(workout_date) = 10
        AND workout_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    started_at           INTEGER NOT NULL DEFAULT (unixepoch()),
    ended_at             INTEGER,
    paused_at            INTEGER,
    paused_seconds       INTEGER NOT NULL DEFAULT 0 CHECK (paused_seconds >= 0),
    revision             INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    notes                TEXT,
    created_at           INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at           INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK (
        (status = 'in_progress' AND paused_at IS NULL AND ended_at IS NULL) OR
        (status = 'paused' AND paused_at IS NOT NULL AND ended_at IS NULL) OR
        (status = 'completed' AND ended_at IS NOT NULL AND paused_at IS NULL)
    )
);
CREATE UNIQUE INDEX fit_workout_one_active
    ON fit_workout((1)) WHERE status IN ('in_progress', 'paused');
CREATE INDEX fit_workout_history ON fit_workout(ended_at DESC, id DESC)
    WHERE status = 'completed';
CREATE INDEX fit_workout_completed_date
    ON fit_workout(workout_date, id)
    WHERE status = 'completed';

-- Exercise names/tracking modes and targets are snapshots. Editing a plan or
-- exercise never rewrites an in-progress or historical workout.
CREATE TABLE fit_workout_exercise (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    workout_id               INTEGER NOT NULL REFERENCES fit_workout(id) ON DELETE CASCADE,
    exercise_id              INTEGER REFERENCES fit_exercise(id) ON DELETE SET NULL,
    exercise_name_snapshot   TEXT NOT NULL,
    tracking_mode_snapshot   TEXT NOT NULL,
    position                 INTEGER NOT NULL CHECK (position >= 0),
    notes                    TEXT,
    UNIQUE (workout_id, position)
);

CREATE TABLE fit_workout_set (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    workout_exercise_id  INTEGER NOT NULL REFERENCES fit_workout_exercise(id) ON DELETE CASCADE,
    position             INTEGER NOT NULL CHECK (position >= 0),
    target_reps          INTEGER CHECK (target_reps > 0),
    target_weight_g      INTEGER CHECK (target_weight_g >= 0),
    target_duration_s    INTEGER CHECK (target_duration_s > 0),
    target_distance_m    INTEGER CHECK (target_distance_m > 0),
    target_rpe_x10       INTEGER CHECK (target_rpe_x10 BETWEEN 10 AND 100),
    actual_reps          INTEGER CHECK (actual_reps > 0),
    actual_weight_g      INTEGER CHECK (actual_weight_g >= 0),
    actual_duration_s    INTEGER CHECK (actual_duration_s > 0),
    actual_distance_m    INTEGER CHECK (actual_distance_m > 0),
    actual_rpe_x10       INTEGER CHECK (actual_rpe_x10 BETWEEN 10 AND 100),
    set_type             TEXT NOT NULL DEFAULT 'working'
                             CHECK (set_type IN ('warmup', 'working', 'drop', 'failure')),
    status               TEXT NOT NULL DEFAULT 'pending'
                             CHECK (status IN ('pending', 'completed', 'skipped')),
    rest_seconds         INTEGER NOT NULL DEFAULT 90 CHECK (rest_seconds BETWEEN 0 AND 3600),
    completed_at         INTEGER,
    UNIQUE (workout_exercise_id, position),
    CHECK ((status = 'completed' AND completed_at IS NOT NULL) OR
           (status <> 'completed' AND completed_at IS NULL))
);

CREATE TABLE fit_body_measurement (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    measured_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    weight_g           INTEGER CHECK (weight_g > 0),
    body_fat_bp        INTEGER CHECK (body_fat_bp BETWEEN 1 AND 10000),
    waist_mm           INTEGER CHECK (waist_mm > 0),
    notes              TEXT,
    created_at         INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK (weight_g IS NOT NULL OR body_fat_bp IS NOT NULL OR waist_mm IS NOT NULL)
);
CREATE INDEX fit_body_measurement_date
    ON fit_body_measurement(measured_at DESC, id DESC);

CREATE TABLE fit_personal_record (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    exercise_id     INTEGER NOT NULL REFERENCES fit_exercise(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('max_weight', 'estimated_1rm')),
    value_g         INTEGER NOT NULL CHECK (value_g > 0),
    workout_set_id  INTEGER NOT NULL REFERENCES fit_workout_set(id) ON DELETE CASCADE,
    achieved_at     INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (exercise_id, kind)
);
CREATE INDEX fit_personal_record_rank
    ON fit_personal_record(exercise_id, kind, value_g DESC);
