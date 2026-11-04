use crate::{Result, Store};
use reel_core::format::now;
use sqlx::Row;
use std::collections::HashSet;

impl Store {
    /// Stars a clip for a person, once however often; false when the clip is not there.
    pub async fn star(&self, user_id: &str, clip_id: &str) -> Result<bool> {
        let r = sqlx::query(
            "INSERT OR IGNORE INTO star (user_id, clip_id, created_at) \
             SELECT ?, id, ? FROM clip WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(user_id)
        .bind(now())
        .bind(clip_id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0 || self.is_starred(user_id, clip_id).await?)
    }

    pub async fn unstar(&self, user_id: &str, clip_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM star WHERE user_id = ? AND clip_id = ?")
            .bind(user_id)
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn is_starred(&self, user_id: &str, clip_id: &str) -> Result<bool> {
        Ok(
            sqlx::query("SELECT 1 FROM star WHERE user_id = ? AND clip_id = ?")
                .bind(user_id)
                .bind(clip_id)
                .fetch_optional(&self.pool)
                .await?
                .is_some(),
        )
    }

    /// The clips a person starred, for the cards of a page.
    pub async fn starred_ids(&self, user_id: &str) -> Result<HashSet<String>> {
        Ok(sqlx::query("SELECT clip_id FROM star WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| r.get::<String, _>(0))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::models::{Clip, User};
    use crate::Store;

    #[tokio::test]
    async fn each_person_has_their_own_stars() {
        let s = Store::open_memory().await.unwrap();
        let (a, b) = (
            User::new("ann", "member", None),
            User::new("bo", "admin", None),
        );
        s.insert_user(&a).await.unwrap();
        s.insert_user(&b).await.unwrap();
        for id in ["c1", "c2"] {
            s.insert_clip(&Clip::new(
                id.into(),
                id.into(),
                "incoming",
                format!("{id}.mp4"),
                None,
            ))
            .await
            .unwrap();
        }
        assert!(s.star(&a.id, "c1").await.unwrap());
        assert!(
            s.star(&a.id, "c1").await.unwrap(),
            "twice is still one star"
        );
        assert!(
            !s.star(&a.id, "nope").await.unwrap(),
            "no star on a clip that is not there"
        );
        s.star(&b.id, "c2").await.unwrap();
        assert_eq!(
            s.starred_ids(&a.id)
                .await
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["c1"]
        );
        assert!(s.is_starred(&b.id, "c2").await.unwrap());
        assert!(
            !s.is_starred(&b.id, "c1").await.unwrap(),
            "ann's star is not bo's"
        );
        s.unstar(&a.id, "c1").await.unwrap();
        assert!(s.starred_ids(&a.id).await.unwrap().is_empty());
        sqlx::query("DELETE FROM clip WHERE id = 'c2'")
            .execute(s.pool())
            .await
            .unwrap();
        assert!(
            !s.is_starred(&b.id, "c2").await.unwrap(),
            "a clip gone takes its stars"
        );
    }
}
