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

INSERT INTO lrn_course (doc_id, name, provider, progress, due_on, tone) VALUES
 ('LRN-C-08', 'System Design · Primer', 'ByteByteGo',  0.72, '2026-05-10', 'blue'),
 ('LRN-C-09', '日本語 · N2 語彙',         'Anki + Deck', 0.44, '2026-06-30', 'rose'),
 ('LRN-C-10', 'Rust · Programming',     'Book',        0.28, '2026-07-15', 'amber'),
 ('LRN-C-11', '金融学导论',             'Coursera',    0.91, '2026-04-28', 'violet');

INSERT INTO lrn_book (doc_id, name, author, status, progress) VALUES
 ('LRN-B-014', '《深度工作》',            'Cal Newport', 'reading', 0.62),
 ('LRN-B-013', '《穷查理宝典》',          'Munger',      'reading', 0.31),
 ('LRN-B-012', '《原则》',                'Ray Dalio',   'done',    1.0),
 ('LRN-B-011', 'Designing Data-Intensive Apps', 'Kleppmann', 'todo', 0);

INSERT INTO lrn_note (doc_id, title, tags, updated_at) VALUES
 ('LRN-N-221', 'System Design · 缓存模式总结', '["System Design","Cache"]', unixepoch('now','-2 hours')),
 ('LRN-N-220', '日语 · 敬语的三层结构',         '["日语","N2"]',           unixepoch('now','-1 day')),
 ('LRN-N-219', '深度工作 · 注意力残留笔记',     '["阅读"]',                 unixepoch('now','-2 days'));

INSERT INTO seq (module, kind, last_value) VALUES
 ('LRN', 'type:C', 11),
 ('LRN', 'type:B', 14),
 ('LRN', 'type:N', 221);
