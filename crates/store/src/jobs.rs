use crate::models::Job;
use crate::{Result, Store};
use reel_core::format::now;
use sqlx::Row;

impl Store {
    pub async fn enqueue(&self, j: &Job) -> Result<()> {
        sqlx::query("INSERT INTO job (id, kind, clip_id, rendition_id, payload, priority, state, attempts, progress, created_at, started_at, finished_at, error, route, cancelled_at, dismissed_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&j.id).bind(&j.kind).bind(&j.clip_id).bind(&j.rendition_id).bind(&j.payload).bind(j.priority).bind(&j.state).bind(j.attempts).bind(j.progress).bind(&j.created_at).bind(&j.started_at).bind(&j.finished_at).bind(&j.error).bind(&j.route).bind(&j.cancelled_at).bind(&j.dismissed_at)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn claim_job(&self) -> Result<Option<Job>> {
        self.claim_job_up_to(i64::MAX).await
    }

    /// The next queued job at this priority or under.
    pub async fn claim_job_up_to(&self, priority: i64) -> Result<Option<Job>> {
        sqlx::query_as::<_, Job>("UPDATE job SET state = 'running', started_at = ?, attempts = attempts + 1 WHERE id = (SELECT id FROM job WHERE state = 'queued' AND priority <= ? ORDER BY priority, created_at LIMIT 1) RETURNING *")
            .bind(now()).bind(priority).fetch_optional(&self.pool).await
    }

    pub async fn job(&self, id: &str) -> Result<Option<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn job_progress(&self, id: &str, progress: f64) -> Result<()> {
        sqlx::query("UPDATE job SET progress = ? WHERE id = ?")
            .bind(progress)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn job_done(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE job SET state = 'done', progress = 1, finished_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn job_failed(&self, id: &str, error: &str) -> Result<()> {
        sqlx::query("UPDATE job SET state = 'failed', finished_at = ?, error = ? WHERE id = ?")
            .bind(now())
            .bind(error)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn active_jobs(&self) -> Result<Vec<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job WHERE state IN ('queued','running') ORDER BY state DESC, priority, created_at").fetch_all(&self.pool).await
    }

    /// A queued job for a rendition runs sooner when something waits for it: its priority
    /// rises to `priority`, never falls (a lower number runs first).
    pub async fn raise_job_priority(&self, rendition_id: &str, priority: i64) -> Result<()> {
        sqlx::query("UPDATE job SET priority = ? WHERE rendition_id = ? AND state = 'queued' AND priority > ?")
            .bind(priority)
            .bind(rendition_id)
            .bind(priority)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn active_job_for_rendition(&self, rendition_id: &str) -> Result<Option<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job WHERE rendition_id = ? AND state IN ('queued','running') ORDER BY created_at DESC LIMIT 1").bind(rendition_id).fetch_optional(&self.pool).await
    }

    pub async fn job_counts(&self) -> Result<(i64, i64)> {
        let row =
            sqlx::query("SELECT SUM(state = 'queued') AS q, SUM(state = 'running') AS r FROM job")
                .fetch_one(&self.pool)
                .await?;
        Ok((
            row.get::<Option<i64>, _>("q").unwrap_or(0),
            row.get::<Option<i64>, _>("r").unwrap_or(0),
        ))
    }

    pub async fn requeue_stuck_jobs(&self) -> Result<u64> {
        let r = sqlx::query(
            "UPDATE job SET state = 'queued', started_at = NULL WHERE state = 'running'",
        )
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    pub async fn recent_jobs(&self, limit: i64) -> Result<Vec<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job ORDER BY created_at DESC LIMIT ?")
            .bind(limit)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn jobs_now(&self) -> Result<Vec<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job WHERE state IN ('queued','running') ORDER BY CASE state WHEN 'running' THEN 0 ELSE 1 END, priority, created_at").fetch_all(&self.pool).await
    }

    pub async fn failed_jobs(&self) -> Result<Vec<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job WHERE state = 'failed' AND dismissed_at IS NULL ORDER BY finished_at DESC").fetch_all(&self.pool).await
    }

    pub async fn finished_jobs(&self, limit: i64) -> Result<Vec<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job WHERE state IN ('done','failed','cancelled') ORDER BY finished_at DESC LIMIT ?").bind(limit).fetch_all(&self.pool).await
    }

    pub async fn cancel_job(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE job SET state = 'cancelled', cancelled_at = ?, finished_at = ? WHERE id = ? AND state IN ('queued','running')")
            .bind(now()).bind(now()).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn dismiss_job(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE job SET dismissed_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn job_route(&self, id: &str, route: &str) -> Result<()> {
        sqlx::query("UPDATE job SET route = ? WHERE id = ?")
            .bind(route)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Queued, running, and failed jobs nobody has dismissed.
    pub async fn job_counts_full(&self) -> Result<(i64, i64, i64)> {
        let row = sqlx::query("SELECT SUM(state = 'queued') AS q, SUM(state = 'running') AS r, SUM(state = 'failed' AND dismissed_at IS NULL) AS f FROM job")
            .fetch_one(&self.pool)
            .await?;
        Ok((
            row.get::<Option<i64>, _>("q").unwrap_or(0),
            row.get::<Option<i64>, _>("r").unwrap_or(0),
            row.get::<Option<i64>, _>("f").unwrap_or(0),
        ))
    }

    pub async fn jobs_of_clip(&self, clip_id: &str) -> Result<Vec<Job>> {
        sqlx::query_as::<_, Job>("SELECT * FROM job WHERE clip_id = ? ORDER BY created_at DESC")
            .bind(clip_id)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn delete_job(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM job WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn purge_finished_jobs(&self, keep: i64) -> Result<u64> {
        let r = sqlx::query("DELETE FROM job WHERE state IN ('done','failed','cancelled') AND id NOT IN (SELECT id FROM job WHERE state IN ('done','failed','cancelled') ORDER BY finished_at DESC LIMIT ?)")
            .bind(keep).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}
