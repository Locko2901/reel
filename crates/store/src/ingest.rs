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

    /// The watcher handed the file to `clip_id`. Its ingest may have finished the file before
    /// this write lands, and then the ingest's state and clip stand.
    pub async fn ingest_file_taken(&self, f: &IngestFile, clip_id: &str) -> Result<()> {
        sqlx::query("UPDATE ingest_file SET size = ?, stable_polls = ?, clip_id = COALESCE(clip_id, ?), state = CASE state WHEN 'seen' THEN 'queued' ELSE state END WHERE path = ?")
            .bind(f.size).bind(f.stable_polls).bind(clip_id).bind(&f.path)
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

#[cfg(test)]
mod tests {
    use crate::models::IngestFile;
    use crate::Store;

    fn seen(path: &str) -> IngestFile {
        IngestFile {
            path: path.into(),
            size: 10,
            first_seen_at: "2026-11-04T20:00:00Z".into(),
            stable_polls: 1,
            state: "seen".into(),
            clip_id: None,
            error: None,
        }
    }

    async fn row(s: &Store, path: &str) -> (String, Option<String>) {
        let f = s.ingest_file(path).await.unwrap().unwrap();
        (f.state, f.clip_id)
    }

    #[tokio::test]
    async fn the_watchers_mark_never_hides_a_finished_ingest() {
        let s = Store::open_memory().await.unwrap();
        let f = seen("a.mp4");
        s.upsert_ingest_file(&f).await.unwrap();
        s.ingest_file_taken(&f, "c1").await.unwrap();
        assert_eq!(row(&s, "a.mp4").await, ("queued".into(), Some("c1".into())));

        // a copy of a clip the library has is dropped within milliseconds, before the mark
        let f = seen("b.mp4");
        s.upsert_ingest_file(&f).await.unwrap();
        s.ingest_file_done_as("b.mp4", "c0").await.unwrap();
        s.ingest_file_taken(&f, "c2").await.unwrap();
        assert_eq!(row(&s, "b.mp4").await, ("done".into(), Some("c0".into())));

        let f = seen("c.mp4");
        s.upsert_ingest_file(&f).await.unwrap();
        s.ingest_file_state("c.mp4", "done", None).await.unwrap();
        s.ingest_file_taken(&f, "c3").await.unwrap();
        assert_eq!(row(&s, "c.mp4").await, ("done".into(), Some("c3".into())));
    }
}
