use crate::models::Rendition;
use crate::{Result, Store};
use reel_core::format::now;
use sqlx::Row;

impl Store {
    pub async fn insert_rendition(&self, r: &Rendition) -> Result<()> {
        sqlx::query("INSERT INTO rendition (id, clip_id, kind, derived_from, recipe, state, path, size, duration, width, height, vcodec, acodec, fps, vbitrate, video_copied, seq, created_at, finished_at, error) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&r.id).bind(&r.clip_id).bind(&r.kind).bind(&r.derived_from).bind(&r.recipe).bind(&r.state).bind(&r.path).bind(r.size)
            .bind(r.duration).bind(r.width).bind(r.height).bind(&r.vcodec).bind(&r.acodec).bind(r.fps).bind(r.vbitrate).bind(r.video_copied)
            .bind(r.seq).bind(&r.created_at).bind(&r.finished_at).bind(&r.error)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn rendition(&self, id: &str) -> Result<Option<Rendition>> {
        sqlx::query_as::<_, Rendition>("SELECT * FROM rendition WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn renditions_of(&self, clip_id: &str) -> Result<Vec<Rendition>> {
        sqlx::query_as::<_, Rendition>(
            "SELECT * FROM rendition WHERE clip_id = ? ORDER BY created_at",
        )
        .bind(clip_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn renditions_derived_from(&self, rendition_id: &str) -> Result<Vec<Rendition>> {
        sqlx::query_as::<_, Rendition>(
            "SELECT * FROM rendition WHERE derived_from = ? AND kind IN ('embed', 'discord', 'web', 'anim')",
        )
        .bind(rendition_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn next_cut_seq(&self, clip_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COALESCE(MAX(seq), 0) + 1 AS n FROM rendition WHERE clip_id = ? AND kind = 'cut'").bind(clip_id).fetch_one(&self.pool).await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn derived_rendition(
        &self,
        from: &str,
        kind: &str,
        recipe: Option<&str>,
    ) -> Result<Option<Rendition>> {
        match recipe {
            Some(r) => sqlx::query_as::<_, Rendition>("SELECT * FROM rendition WHERE derived_from = ? AND kind = ? AND recipe = ? AND state != 'failed' ORDER BY created_at DESC LIMIT 1")
                .bind(from).bind(kind).bind(r).fetch_optional(&self.pool).await,
            None => sqlx::query_as::<_, Rendition>("SELECT * FROM rendition WHERE derived_from = ? AND kind = ? AND state != 'failed' ORDER BY created_at DESC LIMIT 1")
                .bind(from).bind(kind).fetch_optional(&self.pool).await,
        }
    }

    pub async fn rendition_ready(
        &self,
        id: &str,
        path: &str,
        probe: &reel_core::ProbeInfo,
        video_copied: bool,
    ) -> Result<()> {
        sqlx::query("UPDATE rendition SET state = 'ready', path = ?, size = ?, duration = ?, width = ?, height = ?, vcodec = ?, acodec = ?, fps = ?, vbitrate = ?, video_copied = ?, finished_at = ?, error = NULL WHERE id = ?")
            .bind(path).bind(probe.size as i64).bind(probe.duration).bind(probe.width as i64).bind(probe.height as i64)
            .bind(&probe.vcodec).bind(&probe.acodec).bind(probe.fps).bind(probe.vbitrate.map(|b| b as i64)).bind(video_copied as i64).bind(now()).bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn rendition_audio_level(
        &self,
        id: &str,
        loudness: Option<f64>,
        peak: Option<f64>,
    ) -> Result<()> {
        sqlx::query("UPDATE rendition SET audio_loudness = ?, audio_peak = ?, audio_measured_at = ? WHERE id = ?")
            .bind(loudness)
            .bind(peak)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn rendition_reason(&self, id: &str, reason: &str) -> Result<()> {
        sqlx::query("UPDATE rendition SET reason = ? WHERE id = ?")
            .bind(reason)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn rendition_state(&self, id: &str, state: &str, error: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE rendition SET state = ?, error = ?, finished_at = CASE WHEN ? IN ('ready','failed') THEN ? ELSE finished_at END WHERE id = ?")
            .bind(state).bind(error).bind(state).bind(now()).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_rendition(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM rendition WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The bytes of every ready master, cut and embed copy of every clip in the library.
    pub async fn library_bytes(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COALESCE(SUM(r.size), 0) AS bytes FROM rendition r JOIN clip c ON c.id = r.clip_id WHERE r.state = 'ready' AND r.kind IN ('master', 'cut', 'embed') AND c.deleted_at IS NULL")
            .fetch_one(&self.pool).await?;
        Ok(row.get::<i64, _>("bytes"))
    }

    pub async fn ready_versions(&self) -> Result<Vec<Rendition>> {
        sqlx::query_as::<_, Rendition>(
            "SELECT * FROM rendition WHERE state = 'ready' AND kind IN ('master', 'cut')",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn web_renditions(&self) -> Result<Vec<Rendition>> {
        sqlx::query_as::<_, Rendition>(
            "SELECT * FROM rendition WHERE kind = 'web' ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// The newest copy of this kind and recipe made from a rendition, a failed one included.
    pub async fn derived_latest(
        &self,
        from: &str,
        kind: &str,
        recipe: &str,
    ) -> Result<Option<Rendition>> {
        sqlx::query_as::<_, Rendition>("SELECT * FROM rendition WHERE derived_from = ? AND kind = ? AND recipe = ? ORDER BY created_at DESC LIMIT 1")
            .bind(from).bind(kind).bind(recipe).fetch_optional(&self.pool).await
    }

    /// The versions a live link plays: a share's pinned version or its clip's current one, and
    /// the current version of every clip in a collection with a live link.
    pub async fn served_versions(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(&format!(
            "SELECT COALESCE(s.rendition_id, c.current_id) AS rid FROM share s JOIN clip c ON c.id = s.clip_id \
             WHERE s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > ?) AND c.deleted_at IS NULL \
             UNION SELECT c.current_id AS rid FROM collection_link l \
             JOIN collection k ON k.id = l.collection_id JOIN clip c ON {} \
             WHERE l.revoked_at IS NULL AND c.deleted_at IS NULL",
            crate::collections::IN_SET
        ))
        .bind(reel_core::format::now())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .filter_map(|r| r.get::<Option<String>, _>("rid"))
            .collect())
    }

    pub async fn discord_renditions(&self) -> Result<Vec<Rendition>> {
        sqlx::query_as::<_, Rendition>("SELECT * FROM rendition WHERE kind IN ('discord', 'anim') AND state = 'ready' ORDER BY finished_at ASC").fetch_all(&self.pool).await
    }
}
