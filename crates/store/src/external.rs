use crate::models::{CurrentVersion, ExternalFallback, ExternalObject, LiveLink};
use crate::{Result, Store};
use reel_core::format::now;

impl Store {
    pub async fn insert_external_object(&self, o: &ExternalObject) -> Result<()> {
        sqlx::query("INSERT INTO external_object (id, version_id, clip_id, file_id, key, size, class, state, created_at, stored_at, error, exposed) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&o.id).bind(&o.version_id).bind(&o.clip_id).bind(&o.file_id).bind(&o.key).bind(o.size)
            .bind(&o.class).bind(&o.state).bind(&o.created_at).bind(&o.stored_at).bind(&o.error).bind(o.exposed)
            .execute(&self.pool).await?;
        Ok(())
    }

    /// Every object reel put in the storage, or is putting or deleting there.
    pub async fn external_objects(&self) -> Result<Vec<ExternalObject>> {
        sqlx::query_as::<_, ExternalObject>("SELECT * FROM external_object ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
    }

    /// The object a version's links are served from, once it is there.
    pub async fn stored_external_object(&self, version_id: &str) -> Result<Option<ExternalObject>> {
        sqlx::query_as::<_, ExternalObject>(
            "SELECT * FROM external_object WHERE version_id = ? AND state = 'stored' ORDER BY stored_at DESC LIMIT 1",
        )
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn set_external_object_state(
        &self,
        id: &str,
        state: &str,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE external_object SET state = ?, error = ?, stored_at = CASE WHEN ? = 'stored' THEN ? ELSE stored_at END WHERE id = ?")
            .bind(state).bind(error).bind(state).bind(now()).bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn set_external_object_class(&self, id: &str, class: &str) -> Result<()> {
        sqlx::query("UPDATE external_object SET class = ? WHERE id = ?")
            .bind(class)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// A live link serves this object's version: its address is handed out from now on.
    pub async fn expose_external_object(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE external_object SET exposed = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// When the next live share link expires, for the storage to drop its preview then.
    pub async fn next_share_expiry(&self) -> Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT MIN(expires_at) FROM share WHERE revoked_at IS NULL AND expires_at > ?",
        )
        .bind(now())
        .fetch_one(&self.pool)
        .await
    }

    pub async fn delete_external_object(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM external_object WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_external_fallback(
        &self,
        version_id: &str,
        why: &str,
        detail: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO external_fallback (version_id, why, detail, at) VALUES (?,?,?,?) \
                     ON CONFLICT(version_id) DO UPDATE SET why = excluded.why, detail = excluded.detail, \
                     at = CASE WHEN external_fallback.why = excluded.why THEN external_fallback.at ELSE excluded.at END")
            .bind(version_id).bind(why).bind(detail).bind(now())
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn clear_external_fallback(&self, version_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM external_fallback WHERE version_id = ?")
            .bind(version_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn external_fallbacks(&self) -> Result<Vec<ExternalFallback>> {
        sqlx::query_as::<_, ExternalFallback>("SELECT * FROM external_fallback")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn external_fallback(&self, version_id: &str) -> Result<Option<ExternalFallback>> {
        sqlx::query_as::<_, ExternalFallback>(
            "SELECT * FROM external_fallback WHERE version_id = ?",
        )
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Every live link with the version it serves: a share's pinned version or its clip's
    /// current one, and the current version of each clip in a collection with a live link.
    pub async fn live_links(&self) -> Result<Vec<LiveLink>> {
        sqlx::query_as::<_, LiveLink>(&format!(
            "SELECT COALESCE(s.rendition_id, c.current_id) AS version_id, c.id AS clip_id, s.created_at AS created_at, \
             s.last_posted_at AS last_posted_at, s.last_viewed_at AS last_viewed_at \
             FROM share s JOIN clip c ON c.id = s.clip_id \
             WHERE s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > ?) AND c.deleted_at IS NULL AND c.state = 'ready' \
             UNION ALL SELECT c.current_id AS version_id, c.id AS clip_id, l.created_at AS created_at, \
             l.last_posted_at AS last_posted_at, l.last_viewed_at AS last_viewed_at \
             FROM collection_link l JOIN collection k ON k.id = l.collection_id JOIN clip c ON {} \
             WHERE l.revoked_at IS NULL AND c.deleted_at IS NULL AND c.state = 'ready' AND {}",
            crate::collections::IN_SET,
            crate::collections::LINK_SERVES
        ))
        .bind(now())
        .fetch_all(&self.pool)
        .await
        .map(|v| v.into_iter().filter(|l| l.version_id.is_some()).collect())
    }

    /// Every ready clip's current version, its kind and when the clip came in: what the
    /// previews made ahead are chosen from.
    pub async fn current_versions(&self) -> Result<Vec<CurrentVersion>> {
        sqlx::query_as::<_, CurrentVersion>(
            "SELECT c.id AS clip_id, r.id AS version_id, r.kind AS kind, c.created_at AS created_at \
             FROM clip c JOIN rendition r ON r.id = c.current_id \
             WHERE c.deleted_at IS NULL AND c.state = 'ready' AND r.state = 'ready'",
        )
        .fetch_all(&self.pool)
        .await
    }
}
