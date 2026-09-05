//! Machine defaults from `/etc/appsynergy/machine.env` + CLI overrides.
//! Unified installer: `--variant desktop|server` + single or dual-disk RAID1.

use crate::disk::{self, DiskLayout};
use crate::validate;
use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONF: &str = "/etc/appsynergy/machine.env";
pub const CONF_SERVER: &str = "/etc/appsynergy/machine-server.env";
pub const PKGS_DESKTOP: &str = "/etc/appsynergy/packages-target.txt";
pub const PKGS_SERVER: &str = "/etc/appsynergy/packages-target-server.txt";
pub const LOCAL_PKGDIR: &str = "/opt/appsynergy/pkgs";
pub const MNT: &str = "/mnt/appsynergy";
// The [appsynergy] Server URL is package-owned (appsynergy-mirrorlist); the
// installer must never carry a second copy that can drift from it.

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Variant {
    Desktop,
    Server,
}

impl std::fmt::Display for Variant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Variant::Desktop => write!(f, "desktop"),
            Variant::Server => write!(f, "server"),
        }
    }
}

impl Variant {
    pub fn is_server(self) -> bool {
        matches!(self, Variant::Server)
    }

    pub fn pkgs_path(self) -> &'static str {
        match self {
            Variant::Desktop => PKGS_DESKTOP,
            Variant::Server => PKGS_SERVER,
        }
    }

    pub fn product_name(self) -> &'static str {
        match self {
            Variant::Desktop => "AppSynergy Linux",
            Variant::Server => "AppSynergy Server",
        }
    }

    pub fn boot_entry_title(self) -> &'static str {
        match self {
            Variant::Desktop => "AppSynergy Linux",
            Variant::Server => "AppSynergy Server",
        }
    }

    pub fn btrfs_label(self) -> &'static str {
        match self {
            Variant::Desktop => "appsynergy",
            Variant::Server => "appsynergy-server",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum KernelMode {
    Local,
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
    about = "AppSynergy unified full-disk installer (desktop | server)",
    long_about = "DESTROYS ALL DATA on target disk(s).\n\n\
Interactive (default): just run  sudo appsynergy-install\n\
  → pick Desktop or Server, confirm disks, password, SSH key.\n\n\
Batch:  --yes --variant server --disks /dev/nvme0n1,/dev/nvme1n1 \\\n\
        --password-file /tmp/key --ssh-pubkey /path/key.pub\n\
Skip guide: APPSYNERGY_NO_GUIDE=1"
)]
pub struct Cli {
    #[arg(long, env = "APPSYNERGY_VARIANT", value_enum)]
    pub variant: Option<Variant>,

    /// Single target disk (desktop default; server if only one disk).
    /// No clap `env =`: APPSYNERGY_DISK is read in `resolve_disks_from`, below a
    /// typed flag. A clap env binding is indistinguishable from a typed value.
    #[arg(long)]
    pub disk: Option<PathBuf>,

    /// Comma-separated disks for dual NVMe RAID1 (server). Env: APPSYNERGY_DISKS.
    /// Example: /dev/nvme0n1,/dev/nvme1n1
    #[arg(long)]
    pub disks: Option<String>,

    #[arg(long, env = "APPSYNERGY_KERNEL", value_enum)]
    pub kernel: Option<KernelMode>,

    #[arg(long, short = 'y')]
    pub yes: bool,

    #[arg(long, env = "APPSYNERGY_KEYFILE")]
    pub password_file: Option<PathBuf>,

    #[arg(long, env = "APPSYNERGY_HOSTNAME")]
    pub hostname: Option<String>,

    #[arg(long, env = "APPSYNERGY_USER")]
    pub user: Option<String>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub tpm: bool,

    #[arg(long = "no-tpm", action = clap::ArgAction::SetTrue)]
    pub no_tpm: bool,

    #[arg(long, env = "APPSYNERGY_TPM_PCRS", default_value = "7")]
    pub tpm_pcrs: String,

    #[arg(long, env = "APPSYNERGY_SSH_PUBKEY")]
    pub ssh_pubkey: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub variant: Variant,
    pub layout: DiskLayout,
    /// Primary disk (layout.members[0].disk) — convenience.
    pub disk: PathBuf,
    /// Which source named the disks; shown in the banner before the wipe.
    pub disk_source: DiskSource,
    pub efi_part: PathBuf,
    pub luks_part: PathBuf,
    pub cryptname: String,
    pub hostname: String,
    pub user: String,
    pub timezone: String,
    pub locale: String,
    pub keymap: String,
    pub efi_size: String,
    pub kernel: KernelMode,
    pub yes: bool,
    pub password: Option<Vec<u8>>,
    pub mnt: PathBuf,
    pub local_pkgdir: PathBuf,
    pub pkgs_file: PathBuf,
    pub tpm: bool,
    pub tpm_required: bool,
    pub tpm_pcrs: String,
    pub ssh_pubkey: Option<String>,
}

impl Config {
    pub fn load(cli: Cli) -> Result<Self> {
        let env_desktop = load_env_file(Path::new(CONF)).unwrap_or_default();
        let variant = cli.variant.unwrap_or_else(|| {
            parse_variant(
                env_desktop
                    .get("APPSYNERGY_VARIANT")
                    .map(|s| s.as_str())
                    .unwrap_or("desktop"),
            )
        });

        let mut env = env_desktop;
        if variant.is_server() {
            if let Ok(srv) = load_env_file(Path::new(CONF_SERVER)) {
                for (k, v) in srv {
                    env.insert(k, v);
                }
            }
        }

        let (disks, disk_source) = resolve_disks_from(
            cli.disks.as_deref(),
            cli.disk.as_deref(),
            &proc_disk_env(),
            &env,
            &detect_disks(variant),
        )?;
        check_batch_disks(cli.yes, disk_source, &disks)?;
        let efi_size = env.get("APPSYNERGY_EFI_SIZE").cloned().unwrap_or_else(|| {
            if variant.is_server() {
                "1G".into()
            } else {
                "2G".into()
            }
        });
        let cryptname = env
            .get("APPSYNERGY_CRYPTNAME")
            .cloned()
            .unwrap_or_else(|| "cryptroot".into());

        // Desktop: single disk only (ignore accidental second).
        let disks = if !variant.is_server() && disks.len() > 1 {
            eprintln!(
                "WARN: desktop variant uses first disk only ({})",
                disks[0].display()
            );
            vec![disks[0].clone()]
        } else {
            disks
        };

        let layout =
            disk::plan_layout(&disks, &efi_size, &cryptname, variant.btrfs_label(), false)?;

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
            .unwrap_or_else(|| {
                if variant.is_server() {
                    "appsynergy-server".into()
                } else {
                    "appsynergy".into()
                }
            });
        let user = cli
            .user
            .or_else(|| env.get("APPSYNERGY_USER").cloned())
            .unwrap_or_else(|| "appsynergy".into());
        let timezone = env.get("APPSYNERGY_TIMEZONE").cloned().unwrap_or_else(|| {
            if variant.is_server() {
                "UTC".into()
            } else {
                "America/Sao_Paulo".into()
            }
        });
        let locale = env
            .get("APPSYNERGY_LOCALE")
            .cloned()
            .unwrap_or_else(|| "en_US.UTF-8".into());
        let keymap = env
            .get("APPSYNERGY_KEYMAP")
            .cloned()
            .unwrap_or_else(|| "us".into());

        // Every source — CLI flag, process env, machine.env — has converged by
        // here, so this is the one place that has to hold. Downstream these are
        // format!ed into `bash -c` chroot scripts and into /etc files on the
        // target; validating at load means no call site needs quoting to be safe.
        validate::validate_hostname(&hostname)?;
        validate::validate_username(&user)?;
        validate::validate_timezone(&timezone)?;
        validate::check_timezone_exists(&timezone)?;
        validate::validate_locale(&locale)?;
        validate::validate_keymap(&keymap)?;

        let password = match cli.password_file {
            Some(ref p) => Some(read_password_file(p)?),
            None => {
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

        let tpm_env_off = env
            .get("APPSYNERGY_TPM")
            .map(|s| {
                matches!(
                    s.to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                )
            })
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
            (tpm_present, false)
        };

        // Operator pubkey: CLI / env override, else baked live key (existing imma@2048-host).
        // Never generate keys — only install material already on the ISO.
        let ssh_pubkey = match cli.ssh_pubkey {
            Some(ref p) => Some(read_ssh_pubkey(p)?),
            None => {
                if let Some(p) = env.get("APPSYNERGY_SSH_PUBKEY") {
                    let path = PathBuf::from(p);
                    if path.is_file() {
                        Some(read_ssh_pubkey(&path)?)
                    } else if p.contains("ssh-") {
                        Some(p.trim().to_string() + "\n")
                    } else {
                        None
                    }
                } else {
                    None
                }
                .or_else(|| {
                    [
                        "/etc/appsynergy/ssh-unlock.pub", // baked into airootfs
                        "/root/.ssh/authorized_keys",
                        "/root/id_ed25519.pub",
                        "/root/id_rsa.pub",
                    ]
                    .into_iter()
                    .find_map(|cand| {
                        let path = Path::new(cand);
                        path.is_file().then(|| read_ssh_pubkey(path).ok()).flatten()
                    })
                })
            }
        };

        let primary = layout.primary().clone();
        Ok(Self {
            variant,
            disk: primary.disk.clone(),
            disk_source,
            efi_part: primary.efi_part.clone(),
            luks_part: primary.luks_part.clone(),
            cryptname: primary.cryptname.clone(),
            layout,
            hostname,
            user,
            timezone,
            locale,
            keymap,
            efi_size,
            kernel,
            yes: cli.yes,
            password,
            mnt: PathBuf::from(MNT),
            local_pkgdir: PathBuf::from(LOCAL_PKGDIR),
            pkgs_file: PathBuf::from(variant.pkgs_path()),
            tpm,
            tpm_required,
            tpm_pcrs,
            ssh_pubkey,
        })
    }
}

/// Where the target disk list came from. Declared in precedence order, highest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskSource {
    CliDisks,
    CliDisk,
    ProcEnv,
    FileEnv,
    Detected,
}

impl DiskSource {
    /// True when a human or an image baked the value in. Detection is a guess about
    /// unfamiliar hardware, so it is the one source batch mode refuses.
    pub fn is_explicit(self) -> bool {
        !matches!(self, DiskSource::Detected)
    }

    pub fn label(self) -> &'static str {
        match self {
            DiskSource::CliDisks => "--disks",
            DiskSource::CliDisk => "--disk",
            DiskSource::ProcEnv => "process environment",
            DiskSource::FileEnv => "machine.env",
            DiskSource::Detected => "auto-detected",
        }
    }
}

/// Resolve targets and their provenance. Precedence, highest first:
/// `--disks` > `--disk` > process env > machine.env > detection.
/// A typed flag MUST outrank every environment source — otherwise an exported
/// `APPSYNERGY_DISKS` silently redirects `--disk /dev/sda`, which on unfamiliar
/// hardware means wiping the wrong devices.
pub fn resolve_disks_from(
    cli_disks: Option<&str>,
    cli_disk: Option<&Path>,
    proc_env: &HashMap<String, String>,
    file_env: &HashMap<String, String>,
    detected: &[PathBuf],
) -> Result<(Vec<PathBuf>, DiskSource)> {
    if let Some(list) = cli_disks {
        return Ok((disk::parse_disks_list(list)?, DiskSource::CliDisks));
    }
    if let Some(one) = cli_disk {
        return Ok((
            vec![single_disk(&one.to_string_lossy())?],
            DiskSource::CliDisk,
        ));
    }
    for (env, source) in [
        (proc_env, DiskSource::ProcEnv),
        (file_env, DiskSource::FileEnv),
    ] {
        if let Some(list) = non_empty(env.get("APPSYNERGY_DISKS")) {
            return Ok((disk::parse_disks_list(list)?, source));
        }
        if let Some(one) = non_empty(env.get("APPSYNERGY_DISK")) {
            return Ok((vec![single_disk(one)?], source));
        }
    }
    if detected.is_empty() {
        bail!("no target disk found; pass --disks /dev/a,/dev/b or --disk /dev/a");
    }
    Ok((detected.to_vec(), DiskSource::Detected))
}

fn non_empty(v: Option<&String>) -> Option<&str> {
    v.map(|s| s.trim()).filter(|s| !s.is_empty())
}

/// One disk path. A comma here is an operator reaching for the wrong flag; caught at
/// load rather than as "not a block device: /dev/a,/dev/b" after the preflight.
fn single_disk(value: &str) -> Result<PathBuf> {
    if value.contains(',') {
        bail!("--disk takes one disk, got `{value}`: use --disks for multiple disks");
    }
    if !value.starts_with("/dev/") {
        bail!("disk path must start with /dev/: {value}");
    }
    Ok(PathBuf::from(value))
}

/// `--yes` skips every interactive gate, so under batch mode a detected disk list is
/// never confirmed by anyone. Machine.env counts as explicit: that is the baked
/// appliance image, where the disks were chosen when the image was built.
pub fn check_batch_disks(yes: bool, source: DiskSource, disks: &[PathBuf]) -> Result<()> {
    if yes && !source.is_explicit() {
        let names = disks
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(",");
        bail!(
            "--yes requires an explicit --disks/--disk (or APPSYNERGY_DISKS); \
             refusing to wipe auto-detected {names} unconfirmed"
        );
    }
    Ok(())
}

/// The two disk variables from the process environment, read here rather than bound
/// by clap so they rank below a typed flag.
fn proc_disk_env() -> HashMap<String, String> {
    ["APPSYNERGY_DISKS", "APPSYNERGY_DISK"]
        .into_iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect()
}

/// Last resort: the disks that exist right now. Server takes both NVMe as RAID1.
fn detect_disks(variant: Variant) -> Vec<PathBuf> {
    let nvme0 = PathBuf::from("/dev/nvme0n1");
    let nvme1 = PathBuf::from("/dev/nvme1n1");
    if !variant.is_server() {
        return vec![nvme0];
    }
    if nvme0.exists() && nvme1.exists() {
        return vec![nvme0, nvme1];
    }
    if nvme0.exists() {
        return vec![nvme0];
    }
    vec![PathBuf::from("/dev/sda")]
}

fn parse_variant(s: &str) -> Variant {
    match s.to_ascii_lowercase().as_str() {
        "server" | "srv" | "tunnel" => Variant::Server,
        _ => Variant::Desktop,
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

fn read_ssh_pubkey(path: &Path) -> Result<String> {
    let text =
        fs::read_to_string(path).with_context(|| format!("read ssh pubkey {}", path.display()))?;
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !(line.starts_with("ssh-") || line.starts_with("ecdsa-") || line.starts_with("sk-")) {
            bail!(
                "ssh pubkey {}: expected OpenSSH public key line, got: {}",
                path.display(),
                &line[..line.len().min(40)]
            );
        }
        lines.push(line.to_string());
    }
    if lines.is_empty() {
        bail!("ssh pubkey {} has no key lines", path.display());
    }
    Ok(lines.join("\n") + "\n")
}

fn read_password_file(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("read password file {}", path.display()))?;
    disk::strip_password_newline(bytes)
}

pub fn package_list(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if disk::should_skip_pacstrap_pkg(line) {
            eprintln!("    skip pacstrap (post-install path): {line}");
            continue;
        }
        out.push(line.to_string());
    }
    if out.is_empty() {
        bail!("empty package list after filter: {}", path.display());
    }
    Ok(out)
}
