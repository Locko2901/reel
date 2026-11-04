//! The device token: in the Windows Credential Manager; elsewhere a 0600 file in the config
//! folder for the tests and the Xvfb run. It never goes into the settings file or the log.
use std::path::{Path, PathBuf};

pub struct TokenStore {
    dir: PathBuf,
}

impl TokenStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            dir: config_dir.to_path_buf(),
        }
    }

    #[cfg(windows)]
    fn entry() -> anyhow::Result<keyring::Entry> {
        Ok(keyring::Entry::new("reel", "device-token")?)
    }

    #[cfg(windows)]
    pub fn get(&self) -> anyhow::Result<Option<String>> {
        let _ = &self.dir;
        match Self::entry()?.get_password() {
            Ok(t) => Ok(Some(t)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    #[cfg(windows)]
    pub fn set(&self, token: &str) -> anyhow::Result<()> {
        Self::entry()?.set_password(token)?;
        Ok(())
    }

    #[cfg(windows)]
    pub fn clear(&self) -> anyhow::Result<()> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    #[cfg(not(windows))]
    fn file(&self) -> PathBuf {
        self.dir.join("token")
    }

    #[cfg(not(windows))]
    pub fn get(&self) -> anyhow::Result<Option<String>> {
        match std::fs::read_to_string(self.file()) {
            Ok(t) => {
                let t = t.trim().to_string();
                Ok(if t.is_empty() { None } else { Some(t) })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    #[cfg(not(windows))]
    pub fn set(&self, token: &str) -> anyhow::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::create_dir_all(&self.dir)?;
        let part = crate::settings::part_path(&self.file());
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&part)?;
        f.write_all(token.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&part, self.file())?;
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn clear(&self) -> anyhow::Result<()> {
        match std::fs::remove_file(self.file()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_file_stand_in_is_private_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(dir.path());
        assert_eq!(store.get().unwrap(), None);
        store.set("abc123").unwrap();
        assert_eq!(store.get().unwrap().as_deref(), Some("abc123"));
        let mode = std::fs::metadata(dir.path().join("token"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        store.set("def456").unwrap();
        assert_eq!(store.get().unwrap().as_deref(), Some("def456"));
        store.clear().unwrap();
        assert_eq!(store.get().unwrap(), None);
        store.clear().unwrap();
    }
}
