use sqlx::{Sqlite, Transaction};

/// Remove global cross-module references to a document that is being deleted.
///
/// Module-owned tables remain the module's responsibility. This only cleans the
/// shared graph/activity/notification surfaces so dashboards and notification
/// history do not keep dangling links.
pub async fn clear_doc_references(
    tx: &mut Transaction<'_, Sqlite>,
    doc_id: &str,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM module_link WHERE source_doc = ?1 OR target_doc = ?1")
        .bind(doc_id)
        .execute(&mut **tx)
        .await?;

    sqlx::query("UPDATE activity SET link_doc = NULL WHERE link_doc = ?1")
        .bind(doc_id)
        .execute(&mut **tx)
        .await?;

    sqlx::query("UPDATE notification SET doc_ref = NULL WHERE doc_ref = ?1")
        .bind(doc_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Delete a document's own activity rows and clear shared references to it.
///
/// Module-owned domain rows should already be deleted by the caller inside the
/// same transaction. This keeps the common global cleanup sequence consistent
/// across modules.
pub async fn delete_doc_activity_and_references(
    tx: &mut Transaction<'_, Sqlite>,
    module: &str,
    doc_id: &str,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM activity WHERE module = ?1 AND doc_id = ?2")
        .bind(module)
        .bind(doc_id)
        .execute(&mut **tx)
        .await?;
    clear_doc_references(tx, doc_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clear_doc_references_removes_graph_edges_and_activity_links() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE module_link (
                source_doc TEXT NOT NULL,
                target_doc TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'ref',
                PRIMARY KEY (source_doc, target_doc, kind)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE activity (
                module TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                link_doc TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE notification (
                id INTEGER PRIMARY KEY,
                doc_ref TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO module_link (source_doc, target_doc, kind) VALUES
             ('FIN-1', 'LRN-B-1', 'ref'),
             ('LRN-B-1', 'FIT-S-1', 'ref'),
             ('FIN-2', 'FIT-S-1', 'ref')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO notification (id, doc_ref) VALUES
             (1, 'LRN-B-1'),
             (2, 'FIT-S-1')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity (module, doc_id, link_doc) VALUES
             ('FIN', 'FIN-1', 'LRN-B-1'),
             ('LRN', 'LRN-B-1', NULL),
             ('FIN', 'FIN-2', 'FIT-S-1')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        clear_doc_references(&mut tx, "LRN-B-1").await.unwrap();
        tx.commit().await.unwrap();

        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM module_link")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(links, 1);

        let link_doc: Option<String> =
            sqlx::query_scalar("SELECT link_doc FROM activity WHERE doc_id = 'FIN-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(link_doc, None);

        let untouched: Option<String> =
            sqlx::query_scalar("SELECT link_doc FROM activity WHERE doc_id = 'FIN-2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(untouched.as_deref(), Some("FIT-S-1"));

        let cleared_doc_ref: Option<String> =
            sqlx::query_scalar("SELECT doc_ref FROM notification WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cleared_doc_ref, None);

        let untouched_doc_ref: Option<String> =
            sqlx::query_scalar("SELECT doc_ref FROM notification WHERE id = 2")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(untouched_doc_ref.as_deref(), Some("FIT-S-1"));
    }

    #[tokio::test]
    async fn delete_doc_activity_and_references_removes_own_activity_only() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE module_link (
                source_doc TEXT NOT NULL,
                target_doc TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'ref',
                PRIMARY KEY (source_doc, target_doc, kind)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE activity (
                module TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                link_doc TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE notification (
                id INTEGER PRIMARY KEY,
                doc_ref TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO module_link (source_doc, target_doc, kind) VALUES
             ('FIN-1', 'LRN-B-1', 'ref'),
             ('FIT-S-1', 'LRN-B-1', 'ref')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity (module, doc_id, link_doc) VALUES
             ('LRN', 'LRN-B-1', NULL),
             ('FIN', 'LRN-B-1', NULL),
             ('FIN', 'FIN-1', 'LRN-B-1')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO notification (id, doc_ref) VALUES (1, 'LRN-B-1')")
            .execute(&pool)
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        delete_doc_activity_and_references(&mut tx, "LRN", "LRN-B-1")
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let own_activity: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM activity WHERE module = 'LRN'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(own_activity, 0);

        let other_module_same_doc: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity WHERE module = 'FIN' AND doc_id = 'LRN-B-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(other_module_same_doc, 1);

        let linked_activity: Option<String> =
            sqlx::query_scalar("SELECT link_doc FROM activity WHERE doc_id = 'FIN-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(linked_activity, None);

        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM module_link")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(links, 0);

        let doc_ref: Option<String> =
            sqlx::query_scalar("SELECT doc_ref FROM notification WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(doc_ref, None);
    }
}
