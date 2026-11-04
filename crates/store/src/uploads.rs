use crate::models::WebUpload;
use crate::{Result, Store};
use reel_core::format::now;

impl Store {
    pub async fn insert_web_upload(&self, u: &WebUpload) -> Result<()> {
        sqlx::query("INSERT INTO web_upload (id, user_id, file_name, size, file_key, received, state, created_at, updated_at, finished_at, clip_id) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&u.id).bind(&u.user_id).bind(&u.file_name).bind(u.size).bind(&u.file_key).bind(u.received).bind(&u.state)
            .bind(&u.created_at).bind(&u.updated_at).bind(&u.finished_at).bind(&u.clip_id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn web_upload(&self, id: &str) -> Result<Option<WebUpload>> {
        sqlx::query_as::<_, WebUpload>("SELECT * FROM web_upload WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    /// The user's unfinished upload of that file, by its name, size and the browser's key.
    pub async fn open_web_upload(
        &self,
        user_id: &str,
        file_name: &str,
        size: i64,
        file_key: &str,
    ) -> Result<Option<WebUpload>> {
        sqlx::query_as::<_, WebUpload>("SELECT * FROM web_upload WHERE user_id = ? AND file_name = ? AND size = ? AND file_key = ? AND state = 'open' ORDER BY created_at DESC LIMIT 1")
            .bind(user_id).bind(file_name).bind(size).bind(file_key)
            .fetch_optional(&self.pool).await
    }

    pub async fn web_upload_received(&self, id: &str, received: i64) -> Result<()> {
        sqlx::query("UPDATE web_upload SET received = ?, updated_at = ? WHERE id = ?")
            .bind(received)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn web_upload_done(&self, id: &str, clip_id: &str) -> Result<()> {
        let at = now();
        sqlx::query("UPDATE web_upload SET state = 'done', clip_id = ?, finished_at = ?, updated_at = ? WHERE id = ?")
            .bind(clip_id).bind(&at).bind(&at).bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_web_upload(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM web_upload WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Unfinished uploads from the browser nobody touched since the cutoff.
    pub async fn stale_web_uploads(&self, cutoff: &str) -> Result<Vec<WebUpload>> {
        sqlx::query_as::<_, WebUpload>(
            "SELECT * FROM web_upload WHERE state != 'done' AND updated_at < ?",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
    }
}
