mod clips;
mod collections;
mod devices;
mod external;
mod ingest;
mod jobs;
pub mod models;
mod renditions;
mod settings;
mod shares;
mod stars;
pub mod stats;
mod uploads;
mod users;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::time::Duration;

pub type Result<T> = std::result::Result<T, sqlx::Error>;

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Newest,
    Oldest,
    Longest,
    Title,
    Shared,
}

impl Sort {
    pub fn parse(s: &str) -> Self {
        match s {
            "oldest" => Sort::Oldest,
            "longest" => Sort::Longest,
            "title" => Sort::Title,
            "shared" => Sort::Shared,
            _ => Sort::Newest,
        }
    }
    pub fn key(&self) -> &'static str {
        match self {
            Sort::Newest => "newest",
            Sort::Oldest => "oldest",
            Sort::Longest => "longest",
            Sort::Title => "title",
            Sort::Shared => "shared",
        }
    }
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(10))
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn open_memory() -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .in_memory(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// A consistent copy of the database at `path`, taken while reel runs.
    pub async fn backup_to(&self, path: &Path) -> Result<()> {
        sqlx::query("VACUUM INTO ?")
            .bind(path.to_string_lossy().into_owned())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Blanks the values of `keys` in a copy of the database, and their history, so an export
    /// carries no secret; the copy is rewritten so nothing of them stays in its free pages.
    pub async fn blank_settings_in(path: &Path, keys: &[&str]) -> Result<()> {
        use sqlx::Connection;
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .pragma("secure_delete", "ON");
        let mut conn = sqlx::SqliteConnection::connect_with(&opts).await?;
        for key in keys {
            sqlx::query("UPDATE setting SET value = '' WHERE key = ?")
                .bind(key)
                .execute(&mut conn)
                .await?;
            sqlx::query(
                "UPDATE setting_history SET old_value = NULL, new_value = NULL WHERE key = ?",
            )
            .bind(key)
            .execute(&mut conn)
            .await?;
        }
        sqlx::query("VACUUM").execute(&mut conn).await?;
        conn.close().await
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Clip, Job};

    #[tokio::test]
    async fn the_retirement_migration_rewrites_what_the_old_host_left() {
        let opts = SqliteConnectOptions::new()
            .in_memory(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        let all = sqlx::migrate!("./migrations");
        let retirement = all.migrations.iter().position(|m| m.version == 8).unwrap();
        let before = sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(all.migrations[..retirement].to_vec()),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        };
        before.run(&pool).await.unwrap();
        let store = Store { pool: pool.clone() };
        let mut clip = Clip::new(
            "c1".into(),
            "old".into(),
            "fireshare",
            "old.mp4".into(),
            Some("2024-01-01T00:00:00Z".into()),
        );
        clip.recorded_source = Some("fireshare".into());
        sqlx::query("INSERT INTO clip (id, title, description, state, source, source_name, recorded_at, created_at, recorded_source, owner_id) VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(&clip.id).bind(&clip.title).bind(&clip.description).bind(&clip.state).bind(&clip.source).bind(&clip.source_name)
            .bind(&clip.recorded_at).bind(&clip.created_at).bind(&clip.recorded_source).bind(&clip.owner_id)
            .execute(&pool).await.unwrap();
        let job = Job::new(
            "ingest",
            Some("c1".into()),
            None,
            serde_json::json!({ "input": "/fireshare/videos/old.mp4", "source": "fireshare" }),
            0,
        );
        store.enqueue(&job).await.unwrap();
        sqlx::query("INSERT INTO legacy_link (video_id, clip_id, share_id, imported_at) VALUES ('abc', 'c1', NULL, '2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        for key in [
            "share.legacy_links",
            "import.fireshare_db",
            "ingest.poll_seconds",
        ] {
            store.set_setting(key, "7").await.unwrap();
        }

        all.run(&pool).await.unwrap();

        let clip = store.clip("c1").await.unwrap().unwrap();
        assert_eq!(clip.source, "imported");
        assert_eq!(clip.recorded_source.as_deref(), Some("imported"));
        let job = store.job(&job.id).await.unwrap().unwrap();
        let payload: serde_json::Value = serde_json::from_str(&job.payload).unwrap();
        assert_eq!(payload["source"], "imported");
        assert_eq!(
            payload["input"], "/fireshare/videos/old.mp4",
            "only the source word changes"
        );
        assert!(store.setting("share.legacy_links").await.unwrap().is_none());
        assert!(store
            .setting("import.fireshare_db")
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .setting("ingest.poll_seconds")
                .await
                .unwrap()
                .as_deref(),
            Some("7")
        );
        let tables: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(!tables.iter().any(|t| t == "legacy_link"));
    }

    #[tokio::test]
    async fn a_failed_first_admin_leaves_no_transaction_open() {
        let store = Store::open_memory().await.unwrap();
        sqlx::query(
            "CREATE TRIGGER no_users BEFORE INSERT ON user BEGIN SELECT RAISE(ABORT, 'not now'); END",
        )
        .execute(&store.pool)
        .await
        .unwrap();
        let admin = crate::models::User::new("root", "admin", Some("hash".into()));
        assert!(store
            .insert_first_admin(&admin, "setup_code")
            .await
            .is_err());
        sqlx::query("DROP TRIGGER no_users")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&store.pool)
            .await
            .expect("the connection is free for the next write");
        sqlx::query("ROLLBACK").execute(&store.pool).await.unwrap();
        assert!(store
            .insert_first_admin(&admin, "setup_code")
            .await
            .unwrap());
        assert!(!store
            .insert_first_admin(&admin, "setup_code")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn an_identity_gets_a_free_name_and_is_found_by_its_subject() {
        let store = Store::open_memory().await.unwrap();
        let local = crate::models::User::new("mira", "admin", Some("hash".into()));
        store.insert_user(&local).await.unwrap();
        let mut from_provider = crate::models::User::new("Mira", "member", None);
        from_provider.oidc_issuer = Some("https://id.example.org".into());
        from_provider.oidc_subject = Some("sub-1".into());
        let made = store
            .insert_user_freely(from_provider.clone())
            .await
            .unwrap();
        assert_eq!(made.name, "Mira 2", "the local row keeps its name");
        let mut again = from_provider.clone();
        again.id = reel_core::ids::new_id();
        again.oidc_subject = Some("sub-2".into());
        assert_eq!(
            store.insert_user_freely(again).await.unwrap().name,
            "Mira 3"
        );
        let mut long = from_provider.clone();
        long.id = reel_core::ids::new_id();
        long.name = "m".repeat(40);
        long.oidc_subject = Some("sub-3".into());
        store.insert_user(&long).await.unwrap();
        let mut clash = long.clone();
        clash.id = reel_core::ids::new_id();
        clash.oidc_subject = Some("sub-4".into());
        let fitted = store.insert_user_freely(clash).await.unwrap();
        assert_eq!(fitted.name.chars().count(), 40);
        assert!(fitted.name.ends_with(" 2"));
        let found = store
            .user_by_oidc("https://id.example.org", "sub-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, made.id);
        assert!(store
            .user_by_oidc("https://other.example.org", "sub-1")
            .await
            .unwrap()
            .is_none());
        let mut dup = from_provider.clone();
        dup.id = reel_core::ids::new_id();
        dup.name = "someone".into();
        assert!(
            store.insert_user(&dup).await.is_err(),
            "one row per issuer and subject"
        );
    }
}
