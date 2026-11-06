use crate::models::{CapRow, ClipSize};
use crate::models::{Clip, ClipRow};
use crate::{Result, Sort, Store};
use reel_core::format::now;
use sqlx::Row;

fn like_literal(q: &str) -> String {
    q.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

impl Store {
    pub async fn insert_clip(&self, c: &Clip) -> Result<()> {
        sqlx::query("INSERT INTO clip (id, title, description, state, source, source_name, recorded_at, created_at, published_at, master_id, current_id, poster_time, original_path, original_probe, plan_reason, error, deleted_at, source_clip_id, recorded_source, owner_id, source_hash, device_id, game, uploaded_at, uploaded_to, others_footage) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&c.id).bind(&c.title).bind(&c.description).bind(&c.state).bind(&c.source).bind(&c.source_name)
            .bind(&c.recorded_at).bind(&c.created_at).bind(&c.published_at).bind(&c.master_id).bind(&c.current_id)
            .bind(c.poster_time).bind(&c.original_path).bind(&c.original_probe).bind(&c.plan_reason).bind(&c.error).bind(&c.deleted_at).bind(&c.source_clip_id).bind(&c.recorded_source).bind(&c.owner_id)
            .bind(&c.source_hash).bind(&c.device_id).bind(&c.game).bind(&c.uploaded_at).bind(&c.uploaded_to)
            .bind(c.others_footage)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn clip(&self, id: &str) -> Result<Option<Clip>> {
        sqlx::query_as::<_, Clip>("SELECT * FROM clip WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn clips(&self, q: &str, sort: Sort) -> Result<Vec<ClipRow>> {
        let order = match sort {
            Sort::Newest => "COALESCE(c.recorded_at, c.created_at) DESC",
            Sort::Oldest => "COALESCE(c.recorded_at, c.created_at) ASC",
            Sort::Longest => "COALESCE(r.duration, 0) DESC, c.created_at DESC",
            Sort::Title => "c.title COLLATE NOCASE ASC",
            Sort::Shared => "shares DESC, c.created_at DESC",
        };
        let sql = format!(
            "SELECT c.*, r.duration AS duration, r.height AS height, r.width AS width, r.size AS size, r.video_copied AS video_copied, r.id AS rendition_id, \
             (SELECT COUNT(*) FROM share s WHERE s.clip_id = c.id AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > ?)) AS shares, \
             (SELECT progress FROM job j WHERE j.clip_id = c.id AND j.state IN ('queued','running') ORDER BY j.created_at LIMIT 1) AS progress, \
             u.name AS owner_name \
             FROM clip c LEFT JOIN rendition r ON r.id = c.current_id LEFT JOIN user u ON u.id = c.owner_id \
             WHERE c.deleted_at IS NULL AND (? = '' OR lower(c.title) LIKE ? ESCAPE '\\' OR lower(c.description) LIKE ? ESCAPE '\\') ORDER BY {order}"
        );
        let like = format!("%{}%", like_literal(&q.to_ascii_lowercase()));
        sqlx::query_as::<_, ClipRow>(&sql)
            .bind(now())
            .bind(q)
            .bind(&like)
            .bind(&like)
            .fetch_all(&self.pool)
            .await
    }

    /// One clip as the library lists it, with its current version's numbers; None when
    /// it is gone or in the trash.
    pub async fn clip_row(&self, id: &str) -> Result<Option<ClipRow>> {
        let sql = "SELECT c.*, r.duration AS duration, r.height AS height, r.width AS width, r.size AS size, r.video_copied AS video_copied, r.id AS rendition_id, \
             (SELECT COUNT(*) FROM share s WHERE s.clip_id = c.id AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > ?)) AS shares, \
             (SELECT progress FROM job j WHERE j.clip_id = c.id AND j.state IN ('queued','running') ORDER BY j.created_at LIMIT 1) AS progress, \
             u.name AS owner_name \
             FROM clip c LEFT JOIN rendition r ON r.id = c.current_id LEFT JOIN user u ON u.id = c.owner_id \
             WHERE c.id = ? AND c.deleted_at IS NULL";
        sqlx::query_as::<_, ClipRow>(sql)
            .bind(now())
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn detach_spinoffs(&self, clip_id: &str) -> Result<()> {
        sqlx::query("UPDATE clip SET source_clip_id = NULL WHERE source_clip_id = ?")
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn clips_from(&self, source_clip_id: &str) -> Result<Vec<Clip>> {
        sqlx::query_as::<_, Clip>("SELECT * FROM clip WHERE source_clip_id = ? AND deleted_at IS NULL ORDER BY created_at").bind(source_clip_id).fetch_all(&self.pool).await
    }

    pub async fn clip_count_and_size(&self) -> Result<(i64, i64)> {
        let row = sqlx::query("SELECT COUNT(*) AS n, COALESCE(SUM(r.size), 0) AS bytes FROM clip c LEFT JOIN rendition r ON r.id = c.current_id WHERE c.deleted_at IS NULL")
            .fetch_one(&self.pool).await?;
        Ok((row.get::<i64, _>("n"), row.get::<i64, _>("bytes")))
    }

    pub async fn update_clip_text(
        &self,
        id: &str,
        title: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        if let Some(t) = title {
            sqlx::query("UPDATE clip SET title = ? WHERE id = ?")
                .bind(t)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(d) = description {
            sqlx::query("UPDATE clip SET description = ? WHERE id = ?")
                .bind(d)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// The clips made of other clips (montages, cuts saved as their own clip), oldest first.
    pub async fn clips_made_of_clips(&self) -> Result<Vec<Clip>> {
        sqlx::query_as::<_, Clip>("SELECT * FROM clip WHERE source = 'montage' OR source_clip_id IS NOT NULL ORDER BY created_at, id")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn set_others_footage(&self, id: &str, on: bool) -> Result<()> {
        sqlx::query("UPDATE clip SET others_footage = ? WHERE id = ?")
            .bind(on)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_clip_state(&self, id: &str, state: &str, error: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE clip SET state = ?, error = ? WHERE id = ?")
            .bind(state)
            .bind(error)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn publish_clip(
        &self,
        id: &str,
        master_id: &str,
        poster_time: f64,
        original_path: Option<&str>,
        original_probe: &str,
        plan_reason: &str,
    ) -> Result<()> {
        self.publish_clip_showing(
            id,
            master_id,
            (master_id, poster_time),
            original_path,
            original_probe,
            plan_reason,
        )
        .await
    }

    /// Published with another version than the original as the current one, in one write.
    pub async fn publish_clip_showing(
        &self,
        id: &str,
        master_id: &str,
        current: (&str, f64),
        original_path: Option<&str>,
        original_probe: &str,
        plan_reason: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE clip SET state = 'ready', error = NULL, published_at = ?, master_id = ?, current_id = ?, poster_time = ?, original_path = ?, original_probe = ?, plan_reason = ? WHERE id = ?")
            .bind(now()).bind(master_id).bind(current.0).bind(current.1).bind(original_path).bind(original_probe).bind(plan_reason).bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn set_current(
        &self,
        clip_id: &str,
        rendition_id: &str,
        poster_time: f64,
    ) -> Result<()> {
        sqlx::query("UPDATE clip SET current_id = ?, poster_time = ? WHERE id = ?")
            .bind(rendition_id)
            .bind(poster_time)
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_game_if_none(&self, clip_id: &str, game: &str) -> Result<()> {
        sqlx::query("UPDATE clip SET game = ? WHERE id = ? AND (game IS NULL OR game = '')")
            .bind(game)
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The poster is a frame at this time again, no uploaded image.
    pub async fn set_poster_time(&self, clip_id: &str, poster_time: f64) -> Result<()> {
        sqlx::query("UPDATE clip SET poster_time = ?, poster_image = NULL WHERE id = ?")
            .bind(poster_time)
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// An uploaded image is the clip's poster from `at` on.
    pub async fn set_poster_image(&self, clip_id: &str, at: &str) -> Result<()> {
        sqlx::query("UPDATE clip SET poster_image = ? WHERE id = ?")
            .bind(at)
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Every clip with a recording time: its title, the time, its game and its file's name,
    /// what the pass that remakes titles made in UTC looks at.
    pub async fn timed_titles(
        &self,
    ) -> Result<Vec<(String, String, String, Option<String>, String)>> {
        let rows = sqlx::query(
            "SELECT id, title, recorded_at, game, source_name FROM clip WHERE recorded_at IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get("id"),
                    r.get("title"),
                    r.get("recorded_at"),
                    r.get("game"),
                    r.get("source_name"),
                )
            })
            .collect())
    }

    /// The clips without a game whose title has words before a comma, with their recording
    /// time: what the pass that drops an app that is no game from a title looks at.
    pub async fn gameless_titles(&self) -> Result<Vec<(String, String, Option<String>)>> {
        let rows = sqlx::query(
            "SELECT id, title, recorded_at FROM clip WHERE game IS NULL AND title LIKE '%, %'",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get("id"), r.get("title"), r.get("recorded_at")))
            .collect())
    }

    pub async fn set_recorded_at(
        &self,
        clip_id: &str,
        recorded_at: &str,
        source: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE clip SET recorded_at = ?, recorded_source = ? WHERE id = ? AND recorded_at IS NULL").bind(recorded_at).bind(source).bind(clip_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Every live, ready clip with its ready versions' bytes, for the library cap's walk.
    pub async fn clip_sizes_oldest_first(&self) -> Result<Vec<ClipSize>> {
        sqlx::query_as::<_, ClipSize>(
            "SELECT c.id, c.created_at, c.uploaded_at, COALESCE(SUM(r.size), 0) AS bytes FROM clip c LEFT JOIN rendition r ON r.clip_id = c.id AND r.state = 'ready' WHERE c.deleted_at IS NULL AND c.state = 'ready' GROUP BY c.id ORDER BY c.created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Every clip the library cap weighs, oldest first as the library's Oldest sort orders them.
    pub async fn cap_rows(&self) -> Result<Vec<CapRow>> {
        sqlx::query_as::<_, CapRow>(
            "SELECT c.id, c.title, COALESCE(c.recorded_at, c.created_at) AS at, c.uploaded_at, \
             COALESCE((SELECT SUM(r.size) FROM rendition r WHERE r.clip_id = c.id AND r.state = 'ready' AND r.kind IN ('master', 'cut', 'embed')), 0) AS bytes, \
             EXISTS (SELECT 1 FROM share s WHERE s.clip_id = c.id AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > ?)) AS linked, \
             EXISTS (SELECT 1 FROM collection_clip cc JOIN collection k ON k.id = cc.collection_id WHERE cc.clip_id = c.id) AS in_set, \
             EXISTS (SELECT 1 FROM star st WHERE st.clip_id = c.id) AS starred \
             FROM clip c WHERE c.deleted_at IS NULL AND c.state = 'ready' \
             ORDER BY COALESCE(c.recorded_at, c.created_at) ASC, c.created_at ASC",
        )
        .bind(now())
        .fetch_all(&self.pool)
        .await
    }

    /// The clip is on a server now (the desktop app uploaded its master).
    pub async fn mark_uploaded(&self, id: &str, server: &str) -> Result<()> {
        sqlx::query("UPDATE clip SET uploaded_at = ?, uploaded_to = ? WHERE id = ?")
            .bind(now())
            .bind(server)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn trash_clip(&self, id: &str) -> Result<()> {
        let ts = now();
        sqlx::query("UPDATE clip SET deleted_at = ? WHERE id = ?")
            .bind(&ts)
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE share SET revoked_at = COALESCE(revoked_at, ?) WHERE clip_id = ?")
            .bind(&ts)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn restore_clip(&self, id: &str) -> Result<bool> {
        let c = self.clip(id).await?;
        let Some(c) = c else { return Ok(false) };
        let Some(deleted_at) = c.deleted_at else {
            return Ok(false);
        };
        sqlx::query("UPDATE clip SET deleted_at = NULL WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE share SET revoked_at = NULL WHERE clip_id = ? AND revoked_at = ?")
            .bind(id)
            .bind(&deleted_at)
            .execute(&self.pool)
            .await?;
        Ok(true)
    }

    /// Clips that have a folder in the library, with whether each is in the trash: those not
    /// in the trash first, then the newest first.
    pub async fn clips_with_files(&self, limit: i64) -> Result<Vec<(String, bool)>> {
        sqlx::query_as(
            "SELECT id, deleted_at IS NOT NULL FROM clip WHERE master_id IS NOT NULL ORDER BY deleted_at IS NOT NULL, created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    /// Every clip, and whether it is in the trash: what a copy of the library carries.
    pub async fn clip_places(&self) -> Result<Vec<(String, bool)>> {
        sqlx::query_as("SELECT id, deleted_at IS NOT NULL FROM clip ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
    }

    /// The clips with a kept original, and its path as stored.
    pub async fn originals(&self) -> Result<Vec<(String, String)>> {
        sqlx::query_as(
            "SELECT id, original_path FROM clip WHERE original_path IS NOT NULL ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn set_original_path(&self, id: &str, path: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE clip SET original_path = ? WHERE id = ?")
            .bind(path)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Who or what sent a clip to the trash, for the trash page.
    pub async fn set_deleted_by(&self, id: &str, by: &str) -> Result<()> {
        sqlx::query("UPDATE clip SET deleted_by = ? WHERE id = ? AND deleted_at IS NOT NULL")
            .bind(by)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Every clip in the trash with its current version's id, the oldest deleted first: the
    /// first to be purged.
    pub async fn trashed(&self) -> Result<Vec<ClipRow>> {
        let sql = "SELECT c.*, r.duration AS duration, r.height AS height, r.width AS width, r.size AS size, r.video_copied AS video_copied, r.id AS rendition_id, \
             0 AS shares, NULL AS progress, u.name AS owner_name \
             FROM clip c LEFT JOIN rendition r ON r.id = c.current_id LEFT JOIN user u ON u.id = c.owner_id \
             WHERE c.deleted_at IS NOT NULL ORDER BY c.deleted_at ASC";
        sqlx::query_as::<_, ClipRow>(sql)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn trashed_clip(&self, id: &str) -> Result<Option<Clip>> {
        sqlx::query_as::<_, Clip>("SELECT * FROM clip WHERE id = ? AND deleted_at IS NOT NULL")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn trashed_before(&self, cutoff: &str) -> Result<Vec<Clip>> {
        sqlx::query_as::<_, Clip>(
            "SELECT * FROM clip WHERE deleted_at IS NOT NULL AND deleted_at < ?",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn delete_clip_rows(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM job WHERE clip_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM clip WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Keeps the job rows, so the history still tells what happened.
    pub async fn delete_clip_keep_jobs(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM clip WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
