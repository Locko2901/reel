use crate::{Result, Store};
use reel_core::format::now;
use sqlx::FromRow;

/// The library's clips counted by game and month: how many, how long, how much room their
/// versions and previews take. The month is UTC's, from when they were recorded.
#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct StatRow {
    /// The game as the library's filter names it, empty for none.
    pub game: String,
    /// `YYYY-MM`.
    pub month: String,
    pub clips: i64,
    pub seconds: f64,
    pub bytes: i64,
}

#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct WatchedLink {
    pub id: String,
    pub token: String,
    pub label: String,
    pub views: i64,
    pub last_viewed_at: Option<String>,
    pub live: bool,
    pub clip_id: String,
    pub title: String,
}

impl Store {
    /// The library's ready clips by game and month, in one query whatever the library's size.
    pub async fn stat_rows(&self) -> Result<Vec<StatRow>> {
        sqlx::query_as::<_, StatRow>(
            "SELECT COALESCE(NULLIF(TRIM(c.game), ''), '') AS game, \
             substr(COALESCE(c.recorded_at, c.created_at), 1, 7) AS month, \
             COUNT(*) AS clips, \
             COALESCE(SUM(r.duration), 0.0) AS seconds, \
             COALESCE(SUM(s.bytes), 0) AS bytes \
             FROM clip c \
             LEFT JOIN rendition r ON r.id = c.current_id \
             LEFT JOIN (SELECT clip_id, SUM(size) AS bytes FROM rendition WHERE state = 'ready' AND kind IN ('master', 'cut', 'embed') GROUP BY clip_id) s ON s.clip_id = c.id \
             WHERE c.deleted_at IS NULL AND c.state = 'ready' \
             GROUP BY 1, 2",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// The links watched most, their clips in the library, the most first.
    pub async fn most_watched(&self, n: i64) -> Result<Vec<WatchedLink>> {
        sqlx::query_as::<_, WatchedLink>(
            "SELECT s.id, s.token, s.label, s.views, s.last_viewed_at, \
             (s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > ?)) AS live, \
             c.id AS clip_id, c.title \
             FROM share s JOIN clip c ON c.id = s.clip_id \
             WHERE c.deleted_at IS NULL AND s.views > 0 \
             ORDER BY s.views DESC, s.last_viewed_at DESC LIMIT ?",
        )
        .bind(now())
        .bind(n)
        .fetch_all(&self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;
    use std::time::Instant;

    #[tokio::test]
    async fn ten_thousand_clips_in_one_query_under_a_second() {
        let s = Store::open_memory().await.unwrap();
        sqlx::query(
            "WITH RECURSIVE n(v) AS (SELECT 1 UNION ALL SELECT v + 1 FROM n WHERE v < 10000) \
             INSERT INTO clip (id, title, description, state, source, source_name, created_at, recorded_at, game, current_id) \
             SELECT printf('c%05d', v), printf('clip %d', v), '', 'ready', 'incoming', 'x.mp4', '2026-01-01T00:00:00Z', \
             printf('2025-%02d-15T20:00:00Z', v % 12 + 1), CASE v % 5 WHEN 0 THEN NULL ELSE printf('Game %d', v % 5) END, printf('r%05d', v) FROM n",
        )
        .execute(s.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO rendition (id, clip_id, kind, state, size, duration, created_at) \
             SELECT current_id, id, 'master', 'ready', 1000000, 10.0, created_at FROM clip",
        )
        .execute(s.pool())
        .await
        .unwrap();
        let t = Instant::now();
        let rows = s.stat_rows().await.unwrap();
        let took = t.elapsed();
        assert_eq!(rows.iter().map(|r| r.clips).sum::<i64>(), 10_000);
        assert_eq!(
            rows.len(),
            5 * 12,
            "five games (one of them none) over twelve months"
        );
        assert_eq!(
            rows.iter().map(|r| r.bytes).sum::<i64>(),
            10_000 * 1_000_000
        );
        assert!((rows.iter().map(|r| r.seconds).sum::<f64>() - 100_000.0).abs() < 0.1);
        assert!(rows.iter().any(|r| r.game.is_empty()));
        assert!(took.as_millis() < 1000, "{took:?}");
    }
}
