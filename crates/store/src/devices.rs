use crate::models::{Clip, Device, DeviceRow, DeviceUpload, PairCode};
use crate::{Result, Store};
use reel_core::format::now;
use sqlx::Row;

impl Store {
    pub async fn insert_device(&self, d: &Device) -> Result<()> {
        sqlx::query("INSERT INTO device (id, user_id, name, token_hash, created_at, last_seen_at, revoked_at) VALUES (?,?,?,?,?,?,?)")
            .bind(&d.id).bind(&d.user_id).bind(&d.name).bind(&d.token_hash).bind(&d.created_at).bind(&d.last_seen_at).bind(&d.revoked_at)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn device(&self, id: &str) -> Result<Option<Device>> {
        sqlx::query_as::<_, Device>("SELECT * FROM device WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn device_by_token_hash(&self, token_hash: &str) -> Result<Option<Device>> {
        sqlx::query_as::<_, Device>("SELECT * FROM device WHERE token_hash = ?")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await
    }

    /// Every device with its owner's name and how many uploads reached the library.
    pub async fn devices(&self) -> Result<Vec<DeviceRow>> {
        sqlx::query_as::<_, DeviceRow>(
            "SELECT d.*, u.name AS owner_name, \
             (SELECT COUNT(*) FROM device_upload x WHERE x.device_id = d.id AND x.state = 'done') AS uploads \
             FROM device d LEFT JOIN user u ON u.id = d.user_id ORDER BY d.created_at",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn revoke_device(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE device SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn touch_device(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE device SET last_seen_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn insert_pair_code(&self, c: &PairCode) -> Result<()> {
        sqlx::query("INSERT INTO pair_code (code_hash, user_id, created_at, expires_at, used_at, device_id) VALUES (?,?,?,?,?,?)")
            .bind(&c.code_hash).bind(&c.user_id).bind(&c.created_at).bind(&c.expires_at).bind(&c.used_at).bind(&c.device_id)
            .execute(&self.pool).await?;
        Ok(())
    }

    /// Spends the code in one statement: only an unused one that has not expired comes back.
    pub async fn take_pair_code(
        &self,
        code_hash: &str,
        device_id: &str,
    ) -> Result<Option<PairCode>> {
        let at = now();
        sqlx::query_as::<_, PairCode>("UPDATE pair_code SET used_at = ?, device_id = ? WHERE code_hash = ? AND used_at IS NULL AND expires_at > ? RETURNING *")
            .bind(&at).bind(device_id).bind(code_hash).bind(&at)
            .fetch_optional(&self.pool).await
    }

    pub async fn purge_pair_codes_before(&self, cutoff: &str) -> Result<u64> {
        let r = sqlx::query("DELETE FROM pair_code WHERE expires_at <= ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    pub async fn insert_upload(&self, u: &DeviceUpload) -> Result<()> {
        sqlx::query("INSERT INTO device_upload (id, device_id, user_id, file_name, size, sha256, received, state, recorded_at, game, title, created_at, updated_at, finished_at, clip_id, error, keep_last, audio_names) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&u.id).bind(&u.device_id).bind(&u.user_id).bind(&u.file_name).bind(u.size).bind(&u.sha256).bind(u.received).bind(&u.state)
            .bind(&u.recorded_at).bind(&u.game).bind(&u.title).bind(&u.created_at).bind(&u.updated_at).bind(&u.finished_at).bind(&u.clip_id).bind(&u.error).bind(u.keep_last).bind(&u.audio_names)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upload(&self, id: &str) -> Result<Option<DeviceUpload>> {
        sqlx::query_as::<_, DeviceUpload>("SELECT * FROM device_upload WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn open_upload_by_hash(
        &self,
        device_id: &str,
        sha256: &str,
    ) -> Result<Option<DeviceUpload>> {
        sqlx::query_as::<_, DeviceUpload>("SELECT * FROM device_upload WHERE device_id = ? AND sha256 = ? AND state = 'open' ORDER BY created_at DESC LIMIT 1")
            .bind(device_id).bind(sha256).fetch_optional(&self.pool).await
    }

    pub async fn upload_received(&self, id: &str, received: i64) -> Result<()> {
        sqlx::query("UPDATE device_upload SET received = ?, updated_at = ? WHERE id = ?")
            .bind(received)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn upload_done(&self, id: &str, clip_id: &str) -> Result<()> {
        let at = now();
        sqlx::query("UPDATE device_upload SET state = 'done', clip_id = ?, finished_at = ?, updated_at = ? WHERE id = ?")
            .bind(clip_id).bind(&at).bind(&at).bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upload_failed(&self, id: &str, error: &str) -> Result<()> {
        sqlx::query(
            "UPDATE device_upload SET state = 'failed', error = ?, updated_at = ? WHERE id = ?",
        )
        .bind(error)
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_upload(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM device_upload WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Unfinished uploads nobody touched since the cutoff.
    pub async fn stale_uploads(&self, cutoff: &str) -> Result<Vec<DeviceUpload>> {
        sqlx::query_as::<_, DeviceUpload>(
            "SELECT * FROM device_upload WHERE state != 'done' AND updated_at < ?",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
    }

    /// A clip that already holds that raw file, other than the one named: not in the trash,
    /// not failed.
    pub async fn clip_by_source_hash(
        &self,
        sha256: &str,
        except: Option<&str>,
    ) -> Result<Option<Clip>> {
        sqlx::query_as::<_, Clip>("SELECT * FROM clip WHERE source_hash = ? AND id != ? AND deleted_at IS NULL AND state != 'failed' ORDER BY created_at LIMIT 1")
            .bind(sha256).bind(except.unwrap_or("")).fetch_optional(&self.pool).await
    }

    pub async fn set_clip_hash(&self, id: &str, sha256: &str) -> Result<()> {
        sqlx::query("UPDATE clip SET source_hash = ? WHERE id = ?")
            .bind(sha256)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Gives a clip to a device when it has none yet.
    pub async fn adopt_clip(&self, id: &str, device_id: &str) -> Result<()> {
        sqlx::query("UPDATE clip SET device_id = ? WHERE id = ? AND device_id IS NULL")
            .bind(device_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The uploads that pointed at a clip now point at the one that stands for it.
    pub async fn repoint_uploads(&self, from_clip: &str, to_clip: &str) -> Result<()> {
        sqlx::query("UPDATE device_upload SET clip_id = ? WHERE clip_id = ?")
            .bind(to_clip)
            .bind(from_clip)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_clip_game(&self, id: &str, game: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE clip SET game = ? WHERE id = ?")
            .bind(game)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Every clip that holds that raw file: not in the trash, not failed, the oldest first.
    pub async fn clips_by_source_hash(&self, sha256: &str) -> Result<Vec<Clip>> {
        sqlx::query_as::<_, Clip>("SELECT * FROM clip WHERE source_hash = ? AND deleted_at IS NULL AND state != 'failed' ORDER BY created_at")
            .bind(sha256).fetch_all(&self.pool).await
    }

    /// The clips a device uploaded: the ones it brought, and the ones the library had already
    /// when it sent the same file.
    pub async fn clips_of_device(&self, device_id: &str, limit: i64) -> Result<Vec<Clip>> {
        sqlx::query_as::<_, Clip>("SELECT * FROM clip WHERE deleted_at IS NULL AND (device_id = ? OR id IN (SELECT clip_id FROM device_upload WHERE device_id = ? AND state = 'done')) ORDER BY created_at DESC LIMIT ?")
            .bind(device_id).bind(device_id).bind(limit).fetch_all(&self.pool).await
    }

    /// Whether the device uploaded that clip, as `clips_of_device` counts it.
    pub async fn device_has_clip(&self, device_id: &str, clip_id: &str) -> Result<bool> {
        let row = sqlx::query("SELECT EXISTS (SELECT 1 FROM clip WHERE id = ? AND device_id = ?) OR EXISTS (SELECT 1 FROM device_upload WHERE device_id = ? AND clip_id = ? AND state = 'done') AS has")
            .bind(clip_id).bind(device_id).bind(device_id).bind(clip_id)
            .fetch_one(&self.pool).await?;
        Ok(row.get::<bool, _>("has"))
    }

    pub async fn device_counts(&self) -> Result<(i64, i64)> {
        let row = sqlx::query("SELECT COUNT(*) AS n, SUM(CASE WHEN revoked_at IS NULL THEN 1 ELSE 0 END) AS live FROM device")
            .fetch_one(&self.pool).await?;
        Ok((
            row.get::<i64, _>("n"),
            row.get::<Option<i64>, _>("live").unwrap_or(0),
        ))
    }
}
