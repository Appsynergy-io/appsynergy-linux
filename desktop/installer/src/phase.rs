//! Same-boot install journal. A failed run that is re-invoked skips phases
//! already recorded here. The live environment's `/run` is enough: the case
//! that matters is pacstrap dying and the operator running the installer again
//! without rebooting the USB.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const JOURNAL_PATH: &str = "/run/appsynergy-install/journal";

pub struct Journal {
    path: PathBuf,
    done: HashSet<String>,
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
        let _ = fs::remove_file(JOURNAL_PATH);
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
        self.done.insert(id.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
