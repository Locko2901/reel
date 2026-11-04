use crate::models::{ClipRow, Collection, CollectionLink, CollectionRow};
use crate::{Result, Store};
use reel_core::format::now;
use sqlx::Row;
use std::collections::HashMap;

/// Whether clip `c` is in collection `k`: put there by hand, or of the game it is.
pub(crate) const IN_SET: &str = "(CASE WHEN k.game IS NULL THEN EXISTS (SELECT 1 FROM collection_clip cc WHERE cc.collection_id = k.id AND cc.clip_id = c.id) ELSE lower(trim(c.game)) = k.game END)";

impl Store {
    pub async fn insert_collection(&self, c: &Collection) -> Result<()> {
        sqlx::query(
            "INSERT INTO collection (id, name, owner_id, created_at, game) VALUES (?,?,?,?,?)",
        )
        .bind(&c.id)
        .bind(&c.name)
        .bind(&c.owner_id)
        .bind(&c.created_at)
        .bind(&c.game)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn collection(&self, id: &str) -> Result<Option<Collection>> {
        sqlx::query_as::<_, Collection>("SELECT * FROM collection WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    /// A row for every game with clips, named as its newest clip names it; a game without
    /// clips keeps its row, so its link comes back with it.
    pub async fn sync_game_collections(&self) -> Result<()> {
        let games = sqlx::query(
            "SELECT lower(trim(game)) AS k, \
             (SELECT trim(c2.game) FROM clip c2 WHERE lower(trim(c2.game)) = lower(trim(c.game)) AND c2.deleted_at IS NULL ORDER BY c2.created_at DESC LIMIT 1) AS name \
             FROM clip c WHERE deleted_at IS NULL AND trim(coalesce(game, '')) != '' GROUP BY k",
        )
        .fetch_all(&self.pool)
        .await?;
        for g in games {
            let key: String = g.get("k");
            let name: String = g.get("name");
            sqlx::query("INSERT INTO collection (id, name, owner_id, created_at, game) VALUES (?, ?, NULL, ?, ?) ON CONFLICT(game) WHERE game IS NOT NULL DO UPDATE SET name = excluded.name")
                .bind(reel_core::ids::new_id())
                .bind(&name)
                .bind(now())
                .bind(&key)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// Every collection with its live clips, live link and newest clip: the own ones by name,
    /// then the games' by clip count; a game with no clip left is not listed.
    pub async fn collections(&self) -> Result<Vec<CollectionRow>> {
        self.sync_game_collections().await?;
        let rows = sqlx::query_as::<_, CollectionRow>(
            &format!("SELECT k.*, \
             (SELECT COUNT(*) FROM clip c WHERE {IN_SET} AND c.deleted_at IS NULL) AS clips, \
             (SELECT token FROM collection_link l WHERE l.collection_id = k.id AND l.revoked_at IS NULL ORDER BY l.created_at DESC LIMIT 1) AS token, \
             (SELECT MAX(COALESCE(c.recorded_at, c.created_at)) FROM clip c WHERE {IN_SET} AND c.deleted_at IS NULL) AS newest \
             FROM collection k ORDER BY k.game IS NOT NULL, k.name COLLATE NOCASE"),
        )
        .fetch_all(&self.pool)
        .await?;
        let (own, mut games): (Vec<_>, Vec<_>) = rows
            .into_iter()
            .filter(|r| !r.collection.is_game() || r.clips > 0)
            .partition(|r| !r.collection.is_game());
        games.sort_by_key(|g| std::cmp::Reverse(g.clips));
        Ok(own.into_iter().chain(games).collect())
    }

    pub async fn rename_collection(&self, id: &str, name: &str) -> Result<()> {
        sqlx::query("UPDATE collection SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_collection(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM collection WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_to_collection(&self, id: &str, clip_id: &str) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO collection_clip (collection_id, clip_id, added_at) VALUES (?,?,?)")
            .bind(id)
            .bind(clip_id)
            .bind(now())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn remove_from_collection(&self, id: &str, clip_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM collection_clip WHERE collection_id = ? AND clip_id = ?")
            .bind(id)
            .bind(clip_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The clips of a collection in the order they were added, as the library lists them.
    pub async fn collection_clips(&self, id: &str) -> Result<Vec<ClipRow>> {
        sqlx::query_as::<_, ClipRow>(
            &format!("SELECT c.*, r.duration AS duration, r.height AS height, r.width AS width, r.size AS size, r.video_copied AS video_copied, r.id AS rendition_id, \
             (SELECT COUNT(*) FROM share s WHERE s.clip_id = c.id AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > ?)) AS shares, \
             (SELECT progress FROM job j WHERE j.clip_id = c.id AND j.state IN ('queued','running') ORDER BY j.created_at LIMIT 1) AS progress, \
             u.name AS owner_name \
             FROM collection k JOIN clip c ON {IN_SET} LEFT JOIN rendition r ON r.id = c.current_id LEFT JOIN user u ON u.id = c.owner_id \
             WHERE k.id = ? AND c.deleted_at IS NULL \
             ORDER BY CASE WHEN k.game IS NULL THEN (SELECT cc.added_at FROM collection_clip cc WHERE cc.collection_id = k.id AND cc.clip_id = c.id) END, \
             COALESCE(c.recorded_at, c.created_at) DESC, c.id"),
        )
        .bind(now())
        .bind(id)
        .fetch_all(&self.pool)
        .await
    }

    /// The collections a clip is in: the own ones it was put into and its game's.
    pub async fn collections_of_clip(&self, clip_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(&format!(
            "SELECT k.id AS id FROM collection k JOIN clip c ON {IN_SET} WHERE c.id = ?"
        ))
        .bind(clip_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<String, _>("id")).collect())
    }

    pub async fn collection_ids_by_clip(&self) -> Result<HashMap<String, Vec<String>>> {
        let rows = sqlx::query("SELECT clip_id, collection_id FROM collection_clip")
            .fetch_all(&self.pool)
            .await?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for r in rows {
            map.entry(r.get("clip_id"))
                .or_default()
                .push(r.get("collection_id"));
        }
        Ok(map)
    }

    /// Clears the game of every clip whose game is one of `names` (lowercased); how many.
    pub async fn clear_games(&self, names: &[&str]) -> Result<u64> {
        let marks = vec!["?"; names.len()].join(", ");
        let sql = format!("UPDATE clip SET game = NULL WHERE lower(trim(game)) IN ({marks})");
        let mut q = sqlx::query(&sql);
        for n in names {
            q = q.bind(*n);
        }
        Ok(q.execute(&self.pool).await?.rows_affected())
    }

    /// The games of the clips not in the trash with how many each has; None counts the clips
    /// without one.
    pub async fn games(&self) -> Result<Vec<(Option<String>, i64)>> {
        let rows = sqlx::query(
            "SELECT NULLIF(TRIM(game), '') AS g, COUNT(*) AS n FROM clip WHERE deleted_at IS NULL GROUP BY g ORDER BY n DESC, g COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<Option<String>, _>("g"), r.get::<i64, _>("n")))
            .collect())
    }

    pub async fn insert_collection_link(&self, l: &CollectionLink) -> Result<()> {
        sqlx::query("INSERT INTO collection_link (id, collection_id, token, allow_download, created_at, revoked_at, views) VALUES (?,?,?,?,?,?,?)")
            .bind(&l.id)
            .bind(&l.collection_id)
            .bind(&l.token)
            .bind(l.allow_download)
            .bind(&l.created_at)
            .bind(&l.revoked_at)
            .bind(l.views)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn collection_link_by_token(&self, token: &str) -> Result<Option<CollectionLink>> {
        sqlx::query_as::<_, CollectionLink>(
            "SELECT * FROM collection_link WHERE token = ? AND revoked_at IS NULL",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn revoke_collection_links(&self, collection_id: &str) -> Result<()> {
        sqlx::query("UPDATE collection_link SET revoked_at = ? WHERE collection_id = ? AND revoked_at IS NULL")
            .bind(now())
            .bind(collection_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_collection_link_posted(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE collection_link SET last_posted_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn bump_collection_link_views(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE collection_link SET views = views + 1, last_viewed_at = ? WHERE id = ?",
        )
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
