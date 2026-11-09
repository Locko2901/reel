//! Open and Show in folder for a file of the library on this PC, asked by the library's own
//! pages in standalone. A page names a clip and which of its files by id, never a path, and
//! only a file inside the library folder opens.
use crate::library::{inside, plain, real};
use crate::runtime::Runtime;
use reel_core::config::Config;
use reel_core::ids::looks_like_id;
use reel_desktop_core::settings::Mode;
use reel_store::Store;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, State, Webview};
use tauri_plugin_opener::OpenerExt;

const NOT_HERE: &str = "Open and Show in folder are for the library on this PC.";
const NOT_A_FILE: &str = "That is not a file of the library.";
const GONE: &str = "That clip is not in the library any more.";
const NOT_YET: &str = "That file is not made yet.";
const MISSING: &str =
    "The file is not in the library folder any more: it was moved or deleted outside reel.";
const OUTSIDE: &str = "That file is outside the library folder, so it does not open from here.";
const NO_DATABASE: &str = "The library's database did not answer; Settings, Log says why.";

/// Which file of a clip a page asks for.
#[derive(Debug, PartialEq)]
enum Pick {
    /// The current version, the Send panel's "as it is".
    Current,
    /// The raw file a re-encoded clip came from, when it was kept.
    Raw,
    /// One version of the clip, by its id.
    Version(String),
}

impl Pick {
    fn parse(s: &str) -> Option<Pick> {
        match s {
            "current" => Some(Pick::Current),
            "raw" => Some(Pick::Raw),
            id if looks_like_id(id) => Some(Pick::Version(id.into())),
            _ => None,
        }
    }
}

fn down(e: impl std::fmt::Display) -> &'static str {
    tracing::warn!(error = %e, "a library file: the database did not answer");
    NO_DATABASE
}

/// The path the database keeps for a clip's file.
async fn stored(store: &Store, clip: &str, pick: &Pick) -> Result<String, &'static str> {
    let c = store
        .clip(clip)
        .await
        .map_err(down)?
        .filter(|c| c.deleted_at.is_none())
        .ok_or(GONE)?;
    let id = match pick {
        Pick::Raw => return c.original_path.ok_or(NOT_YET),
        Pick::Current => c.current_id.clone().ok_or(NOT_YET)?,
        Pick::Version(id) => id.clone(),
    };
    let r = store
        .rendition(&id)
        .await
        .map_err(down)?
        .filter(|r| r.clip_id == c.id && r.is_version())
        .ok_or(NOT_A_FILE)?;
    if !r.is_ready() {
        return Err(NOT_YET);
    }
    r.path.ok_or(NOT_YET)
}

/// The file with links and `..` resolved, when it is a file inside the library folder.
fn in_library(library: &Path, file: &Path) -> Result<PathBuf, &'static str> {
    let root = std::fs::canonicalize(library)
        .map(|r| plain(&r))
        .map_err(|_| MISSING)?;
    match std::fs::canonicalize(file).map(|f| plain(&f)) {
        Ok(f) if !inside(&f, &root) => Err(OUTSIDE),
        Ok(f) if f.is_file() => Ok(f),
        Ok(_) => Err(MISSING),
        Err(_) if file.components().any(|c| c == Component::ParentDir) => Err(OUTSIDE),
        Err(_) if !inside(&real(file), &root) => Err(OUTSIDE),
        Err(_) => Err(MISSING),
    }
}

/// A clip's file by the ids a page knows, only inside the library folder.
async fn find(
    store: &Store,
    cfg: &Config,
    clip: &str,
    file: &str,
) -> Result<PathBuf, &'static str> {
    let pick = Pick::parse(file)
        .filter(|_| looks_like_id(clip))
        .ok_or(NOT_A_FILE)?;
    let rel = stored(store, clip, &pick).await?;
    in_library(&cfg.data_dir, &cfg.abs(&rel))
}

/// A video or a picture reel keeps: the downloads' list and the raw files it takes in.
fn plays(path: &Path) -> bool {
    crate::downloads::openable(path)
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| ["ts", "avi"].contains(&e.to_ascii_lowercase().as_str()))
}

/// Open (the PC's own player) or Show in folder (the file selected) for a clip's file: the
/// current version, a version by its id, or the raw file kept. Only the library's own page in
/// this app's window asks, and only while the app runs standalone.
#[tauri::command]
pub async fn app_library_open(
    app: AppHandle,
    webview: Webview,
    rt: State<'_, Runtime>,
    clip: String,
    file: String,
    reveal: bool,
) -> Result<(), String> {
    let local = {
        let g = rt.lock();
        g.local
            .clone()
            .filter(|_| g.settings.mode == Mode::Standalone)
    };
    let own = local.as_ref().is_some_and(|l| {
        webview.label() == crate::shell::LIBRARY
            && webview
                .url()
                .ok()
                .and_then(|u| crate::shell::origin_of(u.as_str()))
                == crate::shell::origin_of(&l.url)
    });
    let Some(local) = local.filter(|_| own) else {
        tracing::warn!("Open or Show in folder asked from another page than the library's");
        return Err(NOT_HERE.into());
    };
    let path = match find(&local.ctx.store, &local.ctx.config(), &clip, &file).await {
        Ok(p) => p,
        Err(why) => {
            tracing::warn!(%clip, %file, reveal, why, "a library file did not open");
            return Err(why.into());
        }
    };
    if reveal {
        show_in_folder(&app, &path)?;
    } else if plays(&path) {
        app.opener()
            .open_path(path.display().to_string(), None::<&str>)
            .map_err(|e| format!("The file did not open: {e}"))?;
    } else {
        return Err("Only a video or a picture opens from here; Show in folder has it.".into());
    }
    tracing::info!(%clip, %file, reveal, "a library file shown from its page");
    Ok(())
}

/// Windows selects the file through the shell, Linux through FileManager1 or the portal; where
/// neither answers, the folder opens without the file selected.
fn show_in_folder(app: &AppHandle, path: &Path) -> Result<(), String> {
    let Err(e) = app.opener().reveal_item_in_dir(path) else {
        return Ok(());
    };
    tracing::info!(error = %e, "Show in folder: no file manager selects it, the folder opens");
    let dir = path.parent().unwrap_or(path);
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| format!("The folder did not open: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reel_store::models::{Clip, Rendition};

    const CLIP: &str = "01KA0000000000000000000001";
    const OTHER: &str = "01KA0000000000000000000002";
    const MASTER: &str = "01KA00000000000000000000M1";
    const CUT: &str = "01KA00000000000000000000C1";
    const THEIRS: &str = "01KA00000000000000000000T1";
    const COPY: &str = "01KA00000000000000000000D1";

    fn put(dir: &Path, rel: &str) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"video").unwrap();
        p
    }

    async fn version(store: &Store, id: &str, clip: &str, kind: &str, path: Option<&str>) {
        store
            .insert_rendition(&Rendition::queued(
                id.into(),
                clip.into(),
                kind,
                None,
                None,
                1,
            ))
            .await
            .unwrap();
        if let Some(p) = path {
            store
                .rendition_ready(id, p, &Default::default(), true)
                .await
                .unwrap();
        }
    }

    async fn clip(store: &Store, id: &str) {
        store
            .insert_clip(&Clip::new(
                id.into(),
                "Clip".into(),
                "desktop",
                "Clip.mkv".into(),
                None,
            ))
            .await
            .unwrap();
    }

    #[test]
    fn a_page_names_a_file_by_id_never_by_path() {
        assert_eq!(Pick::parse("current"), Some(Pick::Current));
        assert_eq!(Pick::parse("raw"), Some(Pick::Raw));
        assert_eq!(Pick::parse(CUT), Some(Pick::Version(CUT.into())));
        for path in [
            "",
            "/etc/passwd",
            "../reel.db",
            r"C:\Windows\notepad.exe",
            "library/x/v1.mp4",
            "01KA00000000000000000000C/",
        ] {
            assert_eq!(Pick::parse(path), None, "{path}");
        }
        assert!(plays(Path::new("raw.TS")) && plays(Path::new("v1.mp4")));
        assert!(!plays(Path::new("reel.db")) && !plays(Path::new("run.mp4.sh")));
    }

    #[test]
    fn only_a_file_inside_the_library_folder_is_found() {
        let lib = tempfile::tempdir().unwrap();
        let away = tempfile::tempdir().unwrap();
        let v = put(lib.path(), "library/c/v1.mp4");
        let out = put(away.path(), "elsewhere.mp4");
        assert_eq!(
            in_library(lib.path(), &v),
            Ok(plain(&std::fs::canonicalize(&v).unwrap()))
        );
        assert_eq!(in_library(lib.path(), &out), Err(OUTSIDE));
        let up = lib.path().join("library").join("..").join("..");
        let climbed = up
            .join(away.path().file_name().unwrap())
            .join("elsewhere.mp4");
        assert_eq!(in_library(lib.path(), &climbed), Err(OUTSIDE));
        assert_eq!(
            in_library(lib.path(), &lib.path().join("library/../../gone.mp4")),
            Err(OUTSIDE),
            "a way out that leads nowhere is still a way out"
        );
        assert_eq!(
            in_library(lib.path(), &away.path().join("gone.mp4")),
            Err(OUTSIDE)
        );
        assert_eq!(
            in_library(lib.path(), &lib.path().join("library/c/v2.mp4")),
            Err(MISSING)
        );
        assert_eq!(
            in_library(lib.path(), &lib.path().join("library/c")),
            Err(MISSING),
            "a folder is not a file"
        );
        assert_eq!(
            in_library(&lib.path().join("moved"), &v),
            Err(MISSING),
            "the library folder itself gone"
        );
        #[cfg(unix)]
        {
            let link = lib.path().join("library/c/link.mp4");
            std::os::unix::fs::symlink(&out, &link).unwrap();
            assert_eq!(in_library(lib.path(), &link), Err(OUTSIDE));
        }
    }

    #[tokio::test]
    async fn a_clip_and_its_versions_lead_to_their_files_in_the_library_alone() {
        let lib = tempfile::tempdir().unwrap();
        let away = tempfile::tempdir().unwrap();
        let cfg = Config {
            data_dir: lib.path().to_path_buf(),
            ..Default::default()
        };
        let store = Store::open_memory().await.unwrap();
        clip(&store, CLIP).await;
        clip(&store, OTHER).await;
        let master = put(lib.path(), &format!("library/{CLIP}/v1.mp4"));
        let cut = put(lib.path(), &format!("library/{CLIP}/v2.mp4"));
        let raw = put(lib.path(), "originals/Clip.mkv");
        let theirs = put(away.path(), "theirs.mp4");
        put(lib.path(), "cache/discord/d.mp4");
        version(
            &store,
            MASTER,
            CLIP,
            "master",
            Some(&format!("library/{CLIP}/v1.mp4")),
        )
        .await;
        version(
            &store,
            CUT,
            CLIP,
            "cut",
            Some(&format!("library/{CLIP}/v2.mp4")),
        )
        .await;
        version(&store, COPY, CLIP, "discord", Some("cache/discord/d.mp4")).await;
        version(
            &store,
            THEIRS,
            OTHER,
            "master",
            Some(theirs.to_str().unwrap()),
        )
        .await;
        store
            .publish_clip(CLIP, MASTER, 0.0, Some("originals/Clip.mkv"), "{}", "")
            .await
            .unwrap();
        store
            .publish_clip(OTHER, THEIRS, 0.0, None, "{}", "")
            .await
            .unwrap();
        let found =
            |p: &Path| -> Result<PathBuf, &str> { Ok(plain(&std::fs::canonicalize(p).unwrap())) };

        assert_eq!(find(&store, &cfg, CLIP, "current").await, found(&master));
        assert_eq!(find(&store, &cfg, CLIP, CUT).await, found(&cut));
        assert_eq!(find(&store, &cfg, CLIP, "raw").await, found(&raw));
        store.set_current(CLIP, CUT, 0.0).await.unwrap();
        assert_eq!(find(&store, &cfg, CLIP, "current").await, found(&cut));

        assert_eq!(
            find(&store, &cfg, CLIP, COPY).await,
            Err(NOT_A_FILE),
            "a copy under a size downloads"
        );
        assert_eq!(
            find(&store, &cfg, CLIP, THEIRS).await,
            Err(NOT_A_FILE),
            "another clip's version is not this clip's"
        );
        assert_eq!(
            find(&store, &cfg, OTHER, "current").await,
            Err(OUTSIDE),
            "a path kept outside the library folder"
        );
        assert_eq!(find(&store, &cfg, OTHER, "raw").await, Err(NOT_YET));
        assert_eq!(
            find(&store, &cfg, "../reel.db", "current").await,
            Err(NOT_A_FILE)
        );
        assert_eq!(
            find(&store, &cfg, CLIP, "/etc/passwd").await,
            Err(NOT_A_FILE)
        );

        std::fs::remove_file(&cut).unwrap();
        assert_eq!(find(&store, &cfg, CLIP, "current").await, Err(MISSING));
        std::fs::rename(&raw, lib.path().join("Clip.mkv")).unwrap();
        assert_eq!(find(&store, &cfg, CLIP, "raw").await, Err(MISSING));

        store.trash_clip(CLIP).await.unwrap();
        assert_eq!(find(&store, &cfg, CLIP, MASTER).await, Err(GONE));
        assert_eq!(
            find(&store, &cfg, "01KA0000000000000000000009", "current").await,
            Err(GONE)
        );
    }
}
