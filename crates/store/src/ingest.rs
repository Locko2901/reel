use crate::models::IngestFile;
use crate::{Result, Store};
use reel_core::format::now;

impl Store {
    pub async fn ingest_file(&self, path: &str) -> Result<Option<IngestFile>> {
        sqlx::query_as::<_, IngestFile>("SELECT * FROM ingest_file WHERE path = ?")
            .bind(path)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn upsert_ingest_file(&self, f: &IngestFile) -> Result<()> {
        sqlx::query("INSERT INTO ingest_file (path, size, first_seen_at, stable_polls, state, clip_id, error) VALUES (?,?,?,?,?,?,?) ON CONFLICT(path) DO UPDATE SET size = excluded.size, stable_polls = excluded.stable_polls, state = excluded.state, clip_id = excluded.clip_id, error = excluded.error")
            .bind(&f.path).bind(f.size).bind(&f.first_seen_at).bind(f.stable_polls).bind(&f.state).bind(&f.clip_id).bind(&f.error)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn ingest_files(&self) -> Result<Vec<IngestFile>> {
        sqlx::query_as::<_, IngestFile>("SELECT * FROM ingest_file ORDER BY first_seen_at")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn ingest_file_state(
        &self,
        path: &str,
        state: &str,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE ingest_file SET state = ?, error = ? WHERE path = ?")
            .bind(state)
            .bind(error)
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The file is done, but as a copy of a clip the library had already.
    pub async fn ingest_file_done_as(&self, path: &str, clip_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE ingest_file SET state = 'done', error = NULL, clip_id = ? WHERE path = ?",
        )
        .bind(clip_id)
        .bind(path)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_ingest_file(&self, path: &str) -> Result<()> {
        sqlx::query("DELETE FROM ingest_file WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The book of files a folder scan took, by their full path, so a second scan skips them.
    pub async fn record_import(&self, path: &str, clip_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO import_file (path, clip_id, imported_at) VALUES (?, ?, ?)",
        )
        .bind(path)
        .bind(clip_id)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn imported_already(&self, path: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM import_file WHERE path = ?")
            .bind(path)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    pub async fn delete_ingest_files_of_clip(&self, clip_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM ingest_file WHERE clip_id = ?")
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
