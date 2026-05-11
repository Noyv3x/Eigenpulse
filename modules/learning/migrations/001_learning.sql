CREATE TABLE lrn_course (
    doc_id   TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    provider TEXT,
    progress REAL NOT NULL DEFAULT 0,
    due_on   TEXT,
    tone     TEXT,
    archived INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE lrn_book (
    doc_id   TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    author   TEXT,
    status   TEXT NOT NULL DEFAULT 'reading',
    progress REAL NOT NULL DEFAULT 0
);

CREATE TABLE lrn_note (
    doc_id     TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    body       TEXT,
    tags       TEXT,
    course_doc TEXT REFERENCES lrn_course(doc_id),
    book_doc   TEXT REFERENCES lrn_book(doc_id),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX lrn_note_updated ON lrn_note(updated_at DESC);

INSERT INTO seq (module, kind, last_value) VALUES
 ('LRN', 'type:C', 11),
 ('LRN', 'type:B', 14),
 ('LRN', 'type:N', 221);
