use crate::models::SettingChange;
use crate::{Result, Store};
use reel_core::format::now;
use sqlx::Row;

impl Store {
    pub async fn settings(&self) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT key, value FROM setting ORDER BY key")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<String, _>("key"), r.get::<String, _>("value")))
            .collect())
    }

    pub async fn setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM setting WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value").bind(key).bind(value).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_setting(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM setting WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The library cap waiting after an update: None once it acts, else whether the notification
    /// went and the cap it waited at.
    pub async fn cap_hold(&self) -> Result<Option<(bool, Option<i64>)>> {
        let row = sqlx::query("SELECT told, cap_gb FROM cap_hold WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| {
            (
                r.get::<i64, _>("told") != 0,
                r.get::<Option<i64>, _>("cap_gb"),
            )
        }))
    }

    pub async fn cap_hold_told(&self, cap_gb: i64) -> Result<()> {
        sqlx::query("UPDATE cap_hold SET told = 1, cap_gb = ? WHERE id = 1")
            .bind(cap_gb)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn end_cap_hold(&self) -> Result<()> {
        sqlx::query("DELETE FROM cap_hold")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_setting_history(
        &self,
        key: &str,
        old: Option<&str>,
        new: Option<&str>,
        user: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO setting_history (key, old_value, new_value, user, at) VALUES (?, ?, ?, ?, ?)")
            .bind(key).bind(old).bind(new).bind(user).bind(now())
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn setting_history(&self, limit: i64) -> Result<Vec<SettingChange>> {
        sqlx::query_as::<_, SettingChange>("SELECT * FROM setting_history ORDER BY id DESC LIMIT ?")
            .bind(limit)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn setting_change(&self, id: i64) -> Result<Option<SettingChange>> {
        sqlx::query_as::<_, SettingChange>("SELECT * FROM setting_history WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }
}
