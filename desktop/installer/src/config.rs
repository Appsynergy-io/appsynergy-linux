//! Machine defaults from `/etc/appsynergy/machine.env` + CLI overrides.

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONF: &str = "/etc/appsynergy/machine.env";
pub const PKGS: &str = "/etc/appsynergy/packages-target.txt";
pub const LOCAL_PKGDIR: &str = "/opt/appsynergy/pkgs";
pub const MNT: &str = "/mnt/appsynergy";
pub const APPSYNERGY_REPO: &str =
    "https://git.appsynergy.io/api/packages/imabee/generic/appsynergy-repo/x86_64";

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum KernelMode {
    /// Install from `/opt/appsynergy/pkgs` (linux-appsynergy preferred).
    Local,
    /// Stock Arch `linux` + headers via pacstrap.
    Repo,
}

impl std::fmt::Display for KernelMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KernelMode::Local => write!(f, "local"),
            KernelMode::Repo => write!(f, "repo"),
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "appsynergy-install",
    about = "AppSynergy Linux — destructive full-disk installer (live USB)",
    long_about = "DESTROYS ALL DATA on the target disk.\n\
Environment: see /etc/appsynergy/machine.env\n\
Password file: APPSYNERGY_KEYFILE or --password-file (LUKS + root + user, no trailing newline preferred).\n\
TPM: enrolled during install when a TPM is present (default). Use --no-tpm to skip."
)]
pub struct Cli {
    /// Target block device (full wipe).
    #[arg(long, env = "APPSYNERGY_DISK")]
    pub disk: Option<PathBuf>,

    /// Kernel source: local packages or Arch repo.
    #[arg(long, env = "APPSYNERGY_KERNEL", value_enum)]
    pub kernel: Option<KernelMode>,

    /// Skip interactive disk confirmation.
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// File containing the LUKS + root + user password (shared).
    /// Newline at EOF is stripped. Env: APPSYNERGY_KEYFILE.
    #[arg(long, env = "APPSYNERGY_KEYFILE")]
    pub password_file: Option<PathBuf>,

    /// Hostname override.
    #[arg(long, env = "APPSYNERGY_HOSTNAME")]
    pub hostname: Option<String>,

    /// Login user override.
    #[arg(long, env = "APPSYNERGY_USER")]
    pub user: Option<String>,

    /// Force TPM2 LUKS enrollment (fail install if it cannot complete).
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub tpm: bool,

    /// Skip TPM enrollment even if a TPM is present.
    #[arg(long = "no-tpm", action = clap::ArgAction::SetTrue)]
    pub no_tpm: bool,

    /// PCR bank list for systemd-cryptenroll (default 7 = Secure Boot state).
    /// Env: APPSYNERGY_TPM_PCRS.
    #[arg(long, env = "APPSYNERGY_TPM_PCRS", default_value = "7")]
    pub tpm_pcrs: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub disk: PathBuf,
    pub efi_part: PathBuf,
    pub luks_part: PathBuf,
    pub hostname: String,
    pub user: String,
    pub timezone: String,
    pub locale: String,
    pub keymap: String,
    pub efi_size: String,
    pub cryptname: String,
    pub kernel: KernelMode,
    pub yes: bool,
    /// Shared LUKS + login password (bytes, no trailing newline).
    pub password: Option<Vec<u8>>,
    pub mnt: PathBuf,
    pub local_pkgdir: PathBuf,
    pub pkgs_file: PathBuf,
    /// Attempt TPM2 LUKS enrollment after boot setup.
    pub tpm: bool,
    /// Fail install if TPM enroll was requested but fails.
    pub tpm_required: bool,
    pub tpm_pcrs: String,
}

impl Config {
    pub fn load(cli: Cli) -> Result<Self> {
        let env = load_env_file(Path::new(CONF)).unwrap_or_default();

        let disk = cli
            .disk
            .or_else(|| env.get("APPSYNERGY_DISK").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/dev/nvme0n1"));

        let kernel = cli.kernel.unwrap_or_else(|| {
            match env
                .get("APPSYNERGY_KERNEL")
                .map(|s| s.as_str())
                .unwrap_or("local")
            {
                "repo" => KernelMode::Repo,
                _ => KernelMode::Local,
            }
        });

        let hostname = cli
            .hostname
            .or_else(|| env.get("APPSYNERGY_HOSTNAME").cloned())
            .unwrap_or_else(|| "appsynergy".into());
        let user = cli
            .user
            .or_else(|| env.get("APPSYNERGY_USER").cloned())
            .unwrap_or_else(|| "imma".into());
        let timezone = env
            .get("APPSYNERGY_TIMEZONE")
            .cloned()
            .unwrap_or_else(|| "America/Sao_Paulo".into());
        let locale = env
            .get("APPSYNERGY_LOCALE")
            .cloned()
            .unwrap_or_else(|| "en_US.UTF-8".into());
        let keymap = env
            .get("APPSYNERGY_KEYMAP")
            .cloned()
            .unwrap_or_else(|| "us".into());
        let efi_size = env
            .get("APPSYNERGY_EFI_SIZE")
            .cloned()
            .unwrap_or_else(|| "2G".into());
        let cryptname = env
            .get("APPSYNERGY_CRYPTNAME")
            .cloned()
            .unwrap_or_else(|| "cryptroot".into());

        let (efi_part, luks_part) = partition_names(&disk);

        let password = match cli.password_file {
            Some(ref p) => Some(read_password_file(p)?),
            None => {
                // Also honor default keyfile path used in the live session
                let default = PathBuf::from("/tmp/appsynergy-key");
                if default.is_file() {
                    Some(read_password_file(&default)?)
                } else {
                    None
                }
            }
        };

        let tpm_pcrs = if cli.tpm_pcrs.is_empty() {
            env.get("APPSYNERGY_TPM_PCRS")
                .cloned()
                .unwrap_or_else(|| "7".into())
        } else {
            cli.tpm_pcrs.clone()
        };

        // TPM policy: --no-tpm wins; --tpm requires success; else env; else auto if device present.
        let tpm_env_off = env
            .get("APPSYNERGY_TPM")
            .map(|s| matches!(s.to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off"))
            .unwrap_or(false);
        let tpm_env_on = env
            .get("APPSYNERGY_TPM")
            .map(|s| matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        let tpm_present = Path::new("/dev/tpm0").exists() || Path::new("/dev/tpmrm0").exists();
        let (tpm, tpm_required) = if cli.no_tpm || tpm_env_off {
            (false, false)
        } else if cli.tpm || tpm_env_on {
            (true, true)
        } else {
            // default: enroll when TPM hardware is visible on the live system
            (tpm_present, false)
        };

        Ok(Self {
            disk,
            efi_part,
            luks_part,
            hostname,
            user,
            timezone,
            locale,
            keymap,
            efi_size,
            cryptname,
            kernel,
            yes: cli.yes,
            password,
            mnt: PathBuf::from(MNT),
            local_pkgdir: PathBuf::from(LOCAL_PKGDIR),
            pkgs_file: PathBuf::from(PKGS),
            tpm,
            tpm_required,
            tpm_pcrs,
        })
    }
}

fn partition_names(disk: &Path) -> (PathBuf, PathBuf) {
    let s = disk.to_string_lossy();
    if s.contains("nvme") || s.contains("mmcblk") {
        (PathBuf::from(format!("{s}p1")), PathBuf::from(format!("{s}p2")))
    } else {
        (PathBuf::from(format!("{s}1")), PathBuf::from(format!("{s}2")))
    }
}

fn load_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            map.insert(k.trim().to_string(), v.to_string());
        }
    }
    Ok(map)
}

/// Read password file; strip a single trailing newline (or CRLF).
fn read_password_file(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = fs::read(path).with_context(|| format!("read password file {}", path.display()))?;
    if bytes.is_empty() {
        bail!("password file {} is empty", path.display());
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    if bytes.is_empty() {
        bail!("password file {} empty after stripping newline", path.display());
    }
    Ok(bytes)
}

pub fn package_list(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let skip = [
        "appsynergy-branding",
        "appsynergy-mirrorlist",
        "linux-appsynergy",
        "linux-appsynergy-headers",
        "linux-cachyos-igpu",
        "linux-cachyos-igpu-headers",
    ];
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if skip.contains(&line) {
            eprintln!("    skip pacstrap (post-install path): {line}");
            continue;
        }
        out.push(line.to_string());
    }
    if out.is_empty() {
        bail!("empty package list after filter");
    }
    Ok(out)
}
