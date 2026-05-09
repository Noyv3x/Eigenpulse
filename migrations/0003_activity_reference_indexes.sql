-- Keep shared activity maintenance paths indexed without changing the
-- historical activity table migration.
CREATE INDEX activity_module_doc
    ON activity(module, doc_id);

CREATE INDEX activity_link_doc
    ON activity(link_doc)
    WHERE link_doc IS NOT NULL;
