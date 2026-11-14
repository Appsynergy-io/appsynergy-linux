//! Short-lived credentials, created privately before the first byte is written.
use anyhow::{Context, Result};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub struct SecretFile {
    path: PathBuf,
}
impl SecretFile {
    pub fn new(bytes: &[u8]) -> Result<Self> {
        let mut random = [0u8; 16];
        fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let name: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("appsynergy-secret-{name}"));
        DirBuilder::new().mode(0o700).create(&dir)?;
        let secret = Self {
            path: dir.join("key"),
        };
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&secret.path)
            .context("create private credential")?;
        file.write_all(bytes)?;
        Ok(secret)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for SecretFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        if let Some(dir) = self.path.parent() {
            let _ = fs::remove_dir(dir);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn credentials_are_private_unique_and_removed() {
        let a = SecretFile::new(b"test password").unwrap();
        let b = SecretFile::new(b"other").unwrap();
        let path = a.path().to_owned();
        assert_ne!(path, b.path());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(fs::read(&path).unwrap(), b"test password");
        drop(a);
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }
}
