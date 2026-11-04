use crate::models::{Share, ShareRow};
use crate::{Result, Store};
use reel_core::format::now;

impl Store {
    pub async fn insert_share(&self, s: &Share) -> Result<()> {
        sqlx::query("INSERT INTO share (id, clip_id, token, label, allow_download, created_at, expires_at, revoked_at, views, last_viewed_at, rendition_id) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&s.id).bind(&s.clip_id).bind(&s.token).bind(&s.label).bind(s.allow_download).bind(&s.created_at).bind(&s.expires_at).bind(&s.revoked_at).bind(s.views).bind(&s.last_viewed_at).bind(&s.rendition_id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn share_by_token(&self, token: &str) -> Result<Option<Share>> {
        sqlx::query_as::<_, Share>("SELECT * FROM share WHERE token = ?")
            .bind(token)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn share(&self, id: &str) -> Result<Option<Share>> {
        sqlx::query_as::<_, Share>("SELECT * FROM share WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn shares_of(&self, clip_id: &str) -> Result<Vec<Share>> {
        sqlx::query_as::<_, Share>("SELECT * FROM share WHERE clip_id = ? ORDER BY created_at DESC")
            .bind(clip_id)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn shares_pinned_to(&self, rendition_id: &str) -> Result<Vec<Share>> {
        sqlx::query_as::<_, Share>("SELECT * FROM share WHERE rendition_id = ? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)")
            .bind(rendition_id).bind(now()).fetch_all(&self.pool).await
    }

    pub async fn shares(&self) -> Result<Vec<ShareRow>> {
        sqlx::query_as::<_, ShareRow>(
            "SELECT s.*, c.title AS title, c.current_id AS current_id, c.deleted_at AS clip_deleted_at, \
             p.kind AS pinned_kind, p.seq AS pinned_seq, cur.kind AS cur_kind, cur.seq AS cur_seq \
             FROM share s JOIN clip c ON c.id = s.clip_id \
             LEFT JOIN rendition p ON p.id = s.rendition_id LEFT JOIN rendition cur ON cur.id = c.current_id \
             ORDER BY s.created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn revoke_share(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE share SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// An app fetched the link's page to build its preview: the link was posted somewhere.
    pub async fn mark_posted(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE share SET last_posted_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn bump_views(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE share SET views = views + 1, last_viewed_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
