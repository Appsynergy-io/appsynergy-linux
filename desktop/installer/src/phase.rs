//! Same-boot install journal. A failed run that is re-invoked skips phases
//! already recorded here. The live environment's `/run` is enough: the case
//! that matters is pacstrap dying and the operator running the installer again
//! without rebooting the USB.

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const JOURNAL_PATH: &str = "/run/appsynergy-install/journal";

pub struct Journal {
    path: PathBuf,
    done: HashSet<String>,
}

/// Keep this handle alive for the entire install. Kernel releases the lock on exit.
pub fn lock() -> Result<fs::File> {
    let dir = Path::new("/run/appsynergy-install");
    if !dir.exists() {
        fs::DirBuilder::new().mode(0o700).create(dir)?;
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != 0 || meta.mode() & 0o077 != 0 {
        bail!(
            "{} must be a root-owned private directory (0700)",
            dir.display()
        );
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(dir.join("lock"))?;
    file.try_lock().context("another installer is running")?;
    Ok(file)
}

impl Journal {
    pub fn open() -> Result<Self> {
        Self::open_at(Path::new(JOURNAL_PATH))
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let done = match fs::read_to_string(path) {
            Ok(text) => text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashSet::new(),
            Err(e) => return Err(e).with_context(|| format!("read journal {}", path.display())),
        };
        Ok(Self {
            path: path.to_path_buf(),
            done,
        })
    }

    pub fn reset() -> Result<()> {
        for name in [
            JOURNAL_PATH.to_string(),
            format!("{JOURNAL_PATH}.identity"),
            format!("{JOURNAL_PATH}.active"),
            "/run/appsynergy-install/storage".into(),
            "/run/appsynergy-install/pacman-interrupted".into(),
        ] {
            match fs::remove_file(name) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Never trust a journal from another target, configuration, or installer build.
    pub fn bind(&self, identity: &str) -> Result<()> {
        let path = self.path.with_extension("identity");
        match fs::read_to_string(&path) {
            Ok(old) if old != identity => bail!("resume target/configuration changed; restore the original arguments or unmount/close the old installation and use --fresh"),
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && self.done.is_empty() => {
                if let Some(dir) = path.parent() { fs::create_dir_all(dir)?; }
                let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
                f.write_all(identity.as_bytes())?;
                f.sync_all()?;
            }
            Err(e) => return Err(e).context("resume identity missing; refusing an unbound journal (use --fresh only for a deliberate reinstall)"),
        }
        let active = self.path.with_extension("active");
        if let Ok(id) = fs::read_to_string(&active) {
            if ["partition", "luks", "filesystems", "subvolumes"].contains(&id.as_str())
                && !self.is_done(&id)
            {
                bail!("disk setup was interrupted during {id}; refusing to repeat destructive work. Unmount the target, close its LUKS mappings, then use --fresh to reinstall");
            }
        }
        Ok(())
    }

    pub fn is_done(&self, id: &str) -> bool {
        self.done.contains(id)
    }

    pub fn completed(&self) -> usize {
        self.done.len()
    }

    pub fn run(&mut self, id: &str, f: impl FnOnce() -> Result<()>) -> Result<()> {
        if self.is_done(id) {
            println!("==> {id} (already done, skipping)");
            return Ok(());
        }
        println!("==> {id}");
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let active = self.path.with_extension("active");
        fs::write(&active, id)?;
        f().with_context(|| format!("step `{id}` failed"))?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("open journal {}", self.path.display()))?;
        writeln!(f, "{id}").with_context(|| format!("write journal {}", self.path.display()))?;
        f.sync_all()?;
        self.done.insert(id.to_string());
        fs::remove_file(active)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn exclusive_lock_is_released_when_handle_closes() {
        let secret = crate::secret::SecretFile::new(b"lock test").unwrap();
        let first = fs::OpenOptions::new()
            .write(true)
            .open(secret.path())
            .unwrap();
        let second = fs::OpenOptions::new()
            .write(true)
            .open(secret.path())
            .unwrap();
        first.try_lock().unwrap();
        assert!(second.try_lock().is_err());
        drop(first);
        second.try_lock().unwrap();
    }

    #[test]
    fn resume_rejects_changed_identity_and_unbound_journals() {
        let secret = crate::secret::SecretFile::new(b"fixture").unwrap();
        let path = secret.path().parent().unwrap().join("journal");
        let mut journal = Journal::open_at(&path).unwrap();
        journal.bind("disk A, server").unwrap();
        journal.run("pacstrap", || Ok(())).unwrap();
        let resumed = Journal::open_at(&path).unwrap();
        resumed.bind("disk A, server").unwrap();
        assert!(resumed.bind("disk B, server").is_err());
        assert!(resumed.bind("disk A, desktop").is_err());
        fs::remove_file(path.with_extension("identity")).unwrap();
        assert!(resumed.bind("disk A, server").is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn interrupted_disk_setup_cannot_be_repeated_but_pacstrap_can() {
        let secret = crate::secret::SecretFile::new(b"fixture").unwrap();
        let path = secret.path().parent().unwrap().join("journal");
        for phase in ["partition", "luks", "filesystems", "subvolumes", "pacstrap"] {
            let mut journal = Journal::open_at(&path).unwrap();
            journal.bind("same target").unwrap();
            assert!(journal.run(phase, || anyhow::bail!("interrupted")).is_err());
            let resumed = Journal::open_at(&path).unwrap();
            assert_eq!(resumed.bind("same target").is_ok(), phase == "pacstrap");
            assert!(!resumed.is_done(phase));
            fs::remove_file(path.with_extension("active")).unwrap();
        }
        fs::remove_file(path.with_extension("identity")).unwrap();
    }

    #[test]
    fn empty_journal_runs_and_records() {
        let dir = std::env::temp_dir().join(format!("as-journal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal");
        let mut j = Journal::open_at(&path).unwrap();
        assert!(!j.is_done("partition"));
        let n = AtomicUsize::new(0);
        j.run("partition", || {
            n.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        assert_eq!(n.load(Ordering::SeqCst), 1);
        assert!(j.is_done("partition"));

        let mut j2 = Journal::open_at(&path).unwrap();
        j2.run("partition", || {
            n.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        assert_eq!(n.load(Ordering::SeqCst), 1, "second run must skip");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_phase_is_not_recorded() {
        let dir = std::env::temp_dir().join(format!("as-journal-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal");
        let mut j = Journal::open_at(&path).unwrap();
        let err = j
            .run("luks", || anyhow::bail!("cryptsetup failed"))
            .unwrap_err();
        assert!(format!("{err:#}").contains("luks"));
        assert!(!j.is_done("luks"));
        let text = fs::read_to_string(&path).unwrap_or_default();
        assert!(!text.contains("luks"));
        let _ = fs::remove_dir_all(&dir);
    }
}
