use crate::models::{SigninAttempt, User};
use crate::{Result, Store};
use reel_core::format::now;
use sqlx::Row;

impl Store {
    pub async fn users(&self) -> Result<Vec<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM user ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn user(&self, id: &str) -> Result<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM user WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn user_by_name(&self, name: &str) -> Result<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM user WHERE name = ? COLLATE NOCASE")
            .bind(name.trim())
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn user_count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM user")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn admin_count(&self) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM user WHERE role = 'admin' AND disabled_at IS NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn insert_user(&self, u: &User) -> Result<()> {
        sqlx::query("INSERT INTO user (id, name, password_hash, role, created_at, disabled_at, last_seen_at, oidc_issuer, oidc_subject) VALUES (?,?,?,?,?,?,?,?,?)")
            .bind(&u.id).bind(&u.name).bind(&u.password_hash).bind(&u.role).bind(&u.created_at).bind(&u.disabled_at).bind(&u.last_seen_at).bind(&u.oidc_issuer).bind(&u.oidc_subject)
            .execute(&self.pool).await?;
        Ok(())
    }

    /// Inserts the user under its name, or under the name with a number when that name is
    /// taken, so an identity the provider vouches for never lands in someone else's row.
    pub async fn insert_user_freely(&self, mut u: User) -> Result<User> {
        let base = u.name.clone();
        for n in 1..=99 {
            if n > 1 {
                let suffix = format!(" {n}");
                let room = 40 - suffix.chars().count();
                let head: String = base.chars().take(room).collect();
                u.name = format!("{}{suffix}", head.trim_end());
            }
            match self.insert_user(&u).await {
                Ok(()) => return Ok(u),
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => continue,
                Err(e) => return Err(e),
            }
        }
        Err(sqlx::Error::Protocol(format!("no free name for {base}")))
    }

    pub async fn user_by_oidc(&self, issuer: &str, subject: &str) -> Result<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM user WHERE oidc_issuer = ? AND oidc_subject = ?")
            .bind(issuer)
            .bind(subject)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn set_password(&self, id: &str, hash: &str) -> Result<()> {
        sqlx::query("UPDATE user SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_role(&self, id: &str, role: &str) -> Result<()> {
        sqlx::query("UPDATE user SET role = ? WHERE id = ?")
            .bind(role)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_disabled(&self, id: &str, disabled: bool) -> Result<()> {
        let at = if disabled { Some(now()) } else { None };
        sqlx::query("UPDATE user SET disabled_at = ? WHERE id = ?")
            .bind(at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn touch_user(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE user SET last_seen_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Every session of this user stops being valid: the version is part of what a session
    /// has to match.
    pub async fn bump_session_version(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE user SET session_version = session_version + 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_signin_attempt(&self, ip: &str, name: &str, ok: bool) -> Result<()> {
        sqlx::query("INSERT INTO signin_attempt (ip, name, at, ok) VALUES (?,?,?,?)")
            .bind(ip)
            .bind(name)
            .bind(now())
            .bind(ok as i64)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn failed_attempts_after(&self, since: &str) -> Result<Vec<SigninAttempt>> {
        sqlx::query_as::<_, SigninAttempt>(
            "SELECT * FROM signin_attempt WHERE ok = 0 AND at > ? ORDER BY at DESC LIMIT 200",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn purge_attempts_before(&self, cutoff: &str) -> Result<u64> {
        let r = sqlx::query("DELETE FROM signin_attempt WHERE at <= ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    pub async fn set_clip_owner(&self, clip_id: &str, owner_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE clip SET owner_id = ? WHERE id = ?")
            .bind(owner_id)
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn owner_counts(&self) -> Result<Vec<(Option<String>, i64)>> {
        let rows = sqlx::query(
            "SELECT owner_id, COUNT(*) AS n FROM clip WHERE deleted_at IS NULL GROUP BY owner_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<Option<String>, _>("owner_id"), r.get::<i64, _>("n")))
            .collect())
    }

    /// Makes the first user inside one write transaction, so two requests at once cannot both
    /// become the admin; false when a user exists already.
    pub async fn insert_first_admin(&self, u: &User, code_key: &str) -> Result<bool> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let n: i64 = sqlx::query("SELECT COUNT(*) AS n FROM user")
            .fetch_one(&mut *tx)
            .await?
            .get("n");
        if n > 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO user (id, name, password_hash, role, created_at, disabled_at, last_seen_at) VALUES (?,?,?,?,?,?,?)")
            .bind(&u.id).bind(&u.name).bind(&u.password_hash).bind(&u.role).bind(&u.created_at).bind(&u.disabled_at).bind(&u.last_seen_at)
            .execute(&mut *tx).await?;
        sqlx::query("DELETE FROM setting WHERE key = ?")
            .bind(code_key)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }
}
