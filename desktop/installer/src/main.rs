//! AppSynergy unified full-disk installer (live USB).
//!
//! Variants: **desktop** (Plasma workstation) | **server** (headless tunnels/OVH).
//! Server dual NVMe: LUKS per disk + btrfs RAID1 (data+metadata).
//! Fixes from INSTALL-PROBLEMS.md (2026-07-23):
//! - os-release: write `/usr/lib/os-release` only; symlink `/etc/os-release`
//! - branding: no pre-seed of package-owned files; `pacman -U --overwrite '*'`
//! - `--password-file` / `APPSYNERGY_KEYFILE` for LUKS + chpasswd
//! - `/etc/vconsole.conf` before first mkinitcpio
//! - `efibootmgr` NVRAM entry for new ESP; drop stale PARTUUIDs
//! - TPM2 LUKS enroll during install (passphrase kept); `--no-tpm` to skip
//! - every failure includes the step name

mod cmd;
mod config;
mod disk;
mod guide;

#[cfg(test)]
mod adversarial_tests;

use anyhow::{bail, Context, Result};
use clap::Parser;
use config::{Cli, Config, KernelMode, Variant, APPSYNERGY_REPO};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("ERROR: {e:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let mut cli = Cli::parse();

    if !is_root() {
        bail!("run as root (sudo appsynergy-install)");
    }

    // Guided mode: a few questions, then the usual confirm YES.
    // Skip with: --yes and flags, or APPSYNERGY_NO_GUIDE=1
    if guide::should_guide(&cli) {
        guide::run(&mut cli)?;
    }

    let cfg = Config::load(cli)?;
    for bin in [
        "sgdisk",
        "cryptsetup",
        "mkfs.fat",
        "mkfs.btrfs",
        "pacstrap",
        "genfstab",
        "arch-chroot",
        "bootctl",
        "blkid",
        "efibootmgr",
    ] {
        cmd::need(bin)?;
    }
    if cfg.tpm {
        // Prefer systemd-cryptenroll; cryptsetup is already required.
        if !cmd::which("systemd-cryptenroll") {
            if cfg.tpm_required {
                bail!("TPM enroll requested but systemd-cryptenroll missing");
            }
            eprintln!("WARN: systemd-cryptenroll missing — TPM step will skip");
        }
    }
    for m in &cfg.layout.members {
        if !m.disk.exists() {
            bail!("not a block device: {}", m.disk.display());
        }
    }
    if !cfg.pkgs_file.is_file() {
        bail!("missing package list: {}", cfg.pkgs_file.display());
    }

    banner(&cfg);
    confirm(&cfg)?;
    warn_network();

    step("partition", || partition(&cfg))?;
    step("luks", || luks_format_open(&cfg))?;
    step("filesystems", || filesystems(&cfg))?;
    step("subvolumes", || btrfs_subvols(&cfg))?;
    if cfg.layout.is_raid1() {
        step("esp-mirror", || mirror_esp(&cfg))?;
    }
    step("pacstrap", || pacstrap_packages(&cfg))?;
    step("local-kernel", || install_local_kernel(&cfg))?;
    step("appsynergy-repo", || register_appsynergy_repo(&cfg))?;
    step("branding", || install_branding(&cfg))?;
    step("browsers", || install_browsers(&cfg))?;
    step("fstab-crypttab", || fstab_crypttab(&cfg))?;
    step("locale", || locale_hostname(&cfg))?;
    step("os-release", || apply_os_release(&cfg))?;
    step("network", || network_setup(&cfg))?;
    step("mkinitcpio-config", || configure_mkinitcpio(&cfg))?;
    step("users", || create_users(&cfg))?;
    // Existing operator pubkey from live ISO → root + user (both variants).
    step("ssh-keys", || install_ssh_keys(&cfg))?;
    step("bootloader", || install_bootloader(&cfg))?;
    step("efibootmgr", || fix_efi_nvram(&cfg))?;
    // Server hooks before initramfs so dropbear + SSH unlock are in the image.
    step("server-overlay", || apply_server_overlay(&cfg))?;
    step("initramfs", || rebuild_initramfs(&cfg))?;
    // TPM after initramfs so sd-encrypt + crypttab exist; rebuild again if enrolled.
    step("tpm-enroll", || enroll_tpm(&cfg))?;
    step("services", || enable_services(&cfg))?;
    step("finalize", || finalize(&cfg))?;

    println!();
    println!("============================================================");
    println!("  INSTALL COMPLETE ({})", cfg.variant);
    println!("  1. reboot  (remove USB)");
    if cfg.layout.is_raid1() {
        println!("  disk: 2× LUKS2 + btrfs RAID1 (data+metadata)");
    } else {
        println!("  disk: LUKS2 full-disk + btrfs");
    }
    if cfg.tpm {
        println!("  2. TPM unlock should open the disk(s) (passphrase still works)");
    } else {
        println!("  2. At unlock prompt: type VOLUME passphrase");
        println!("     (no TPM seen, or --no-tpm; enroll later: appsynergy-tpm-enroll)");
    }
    println!("  3. Login as {}{}", cfg.user, if cfg.variant.is_server() { " (ssh root/key once configured)" } else { "" });
    if cfg.variant.is_server() {
        println!("  4. Edit /etc/systemd/network/*.network if DHCP is wrong");
        println!("  5. Unlock order: TPM → ssh root@ip (initrd) → console — /etc/appsynergy/UNLOCK.txt");
        println!("  6. wg + nft: /etc/nftables.conf; containers: nerdctl/containerd");
    }
    println!("============================================================");
    let closes: Vec<String> = cfg
        .layout
        .members
        .iter()
        .map(|m| format!("cryptsetup close {}", m.cryptname))
        .collect();
    println!(
        "Unmount: umount -R {} && {}",
        cfg.mnt.display(),
        closes.join(" && ")
    );
    Ok(())
}

fn step(name: &str, f: impl FnOnce() -> Result<()>) -> Result<()> {
    println!("==> {name}");
    f().with_context(|| format!("step `{name}` failed"))
}

fn is_root() -> bool {
    cmd::output("root-check", "id", &["-u"])
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1)
        == 0
}

fn banner(cfg: &Config) {
    println!();
    println!("============================================================");
    println!(
        "  {} installer — FULL DISK WIPE (rust)",
        cfg.variant.product_name()
    );
    println!("============================================================");
    println!("  variant:    {}", cfg.variant);
    for (i, m) in cfg.layout.members.iter().enumerate() {
        println!(
            "  disk[{i}]:    {}  EFI={}  LUKS={}  mapper={}",
            m.disk.display(),
            m.efi_part.display(),
            m.luks_part.display(),
            m.cryptname
        );
    }
    if cfg.layout.is_raid1() {
        println!("  btrfs:      RAID1 data+metadata  label={}", cfg.layout.label);
    } else {
        println!("  btrfs:      single  label={}", cfg.layout.label);
    }
    println!("  hostname:   {}", cfg.hostname);
    println!("  user:       {} (bash login; fish installed)", cfg.user);
    println!(
        "  locale:     {}  keymap: {}  tz: {}",
        cfg.locale, cfg.keymap, cfg.timezone
    );
    println!("  kernel:     {} ({})", cfg.kernel, cfg.variant);
    println!("  packages:   {}", cfg.pkgs_file.display());
    println!(
        "  password:   {}",
        if cfg.password.is_some() {
            "from keyfile (non-interactive)"
        } else {
            "interactive prompts"
        }
    );
    println!(
        "  tpm:        {} (pcrs={})",
        if cfg.tpm {
            if cfg.tpm_required {
                "enroll (required)"
            } else {
                "enroll (auto)"
            }
        } else {
            "skip"
        },
        cfg.tpm_pcrs
    );
    if cfg.variant.is_server() {
        println!(
            "  ssh key:    {}",
            if cfg.ssh_pubkey.is_some() {
                "armed (root + initrd unlock)"
            } else {
                "MISSING — pass --ssh-pubkey (SSH unlock + key-only root disarmed)"
            }
        );
        println!("  unlock:     TPM auto → SSH dropbear passphrase → console");
    }
    println!("============================================================");
    let _ = cmd::run(
        "lsblk",
        "lsblk",
        &[
            "-o",
            "NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS",
            &cfg.disk.to_string_lossy(),
        ],
    );
    println!();
}

fn confirm(cfg: &Config) -> Result<()> {
    if cfg.yes {
        return Ok(());
    }
    let expect = cfg
        .layout
        .members
        .iter()
        .map(|m| m.disk.display().to_string())
        .collect::<Vec<_>>()
        .join(",");
    print!("Type disk path(s) exactly to continue ({expect}): ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    if line.trim() != expect {
        bail!("aborted (confirmation mismatch)");
    }
    print!("Type YES to destroy all data: ");
    io::stdout().flush()?;
    line.clear();
    io::stdin().read_line(&mut line)?;
    if line.trim() != "YES" {
        bail!("aborted");
    }
    Ok(())
}

fn warn_network() {
    let ok = cmd::run("ping", "ping", &["-c1", "-W3", "archlinux.org"]).is_ok()
        || cmd::run("ping", "ping", &["-c1", "-W3", "geo.mirror.pkgbuild.com"]).is_ok();
    if !ok {
        eprintln!("WARN: no network ping; attempting pacstrap anyway (may fail)");
    }
}

fn partition(cfg: &Config) -> Result<()> {
    for m in &cfg.layout.members {
        let disk = m.disk.to_string_lossy();
        println!("    partitioning {}", disk);
        let _ = cmd::run("partition", "wipefs", &["-af", &disk]);
        cmd::run("partition", "sgdisk", &["--zap-all", &disk])?;
        cmd::run(
            "partition",
            "sgdisk",
            &[
                &format!("-n1:0:+{}", cfg.efi_size),
                "-t1:EF00",
                "-c1:EFI",
                &disk,
            ],
        )?;
        cmd::run(
            "partition",
            "sgdisk",
            &["-n2:0:0", "-t2:8309", "-c2:cryptroot", &disk],
        )?;
        let _ = cmd::run("partition", "partprobe", &[&disk]);
    }
    thread::sleep(Duration::from_secs(1));
    for m in &cfg.layout.members {
        if !m.efi_part.exists() || !m.luks_part.exists() {
            bail!(
                "partitions not found after sgdisk ({} / {})",
                m.efi_part.display(),
                m.luks_part.display()
            );
        }
    }
    Ok(())
}

fn luks_format_open(cfg: &Config) -> Result<()> {
    for m in &cfg.layout.members {
        let part = m.luks_part.to_string_lossy();
        println!("    LUKS {} → {}", part, m.cryptname);
        if let Some(ref pw) = cfg.password {
            cmd::run_stdin(
                "luks",
                "cryptsetup",
                &[
                    "luksFormat",
                    "--type",
                    "luks2",
                    "--pbkdf",
                    "argon2id",
                    "--batch-mode",
                    "--key-file=-",
                    &part,
                ],
                pw,
            )?;
            cmd::run_stdin(
                "luks",
                "cryptsetup",
                &["open", "--key-file=-", &part, &m.cryptname],
                pw,
            )?;
        } else {
            println!("    Type NEW volume passphrase TWICE for {}", part);
            cmd::run(
                "luks",
                "cryptsetup",
                &[
                    "luksFormat",
                    "--type",
                    "luks2",
                    "--pbkdf",
                    "argon2id",
                    &part,
                ],
            )?;
            cmd::run("luks", "cryptsetup", &["open", &part, &m.cryptname])?;
        }
    }
    Ok(())
}

fn filesystems(cfg: &Config) -> Result<()> {
    for (i, m) in cfg.layout.members.iter().enumerate() {
        let label = if i == 0 { "EFI" } else { "EFI2" };
        cmd::run(
            "filesystems",
            "mkfs.fat",
            &["-F32", "-n", label, &m.efi_part.to_string_lossy()],
        )?;
    }

    let mappers: Vec<String> = cfg
        .layout
        .members
        .iter()
        .map(|m| m.mapper_path().display().to_string())
        .collect();
    let mut args = vec!["-f".to_string(), "-L".to_string(), cfg.layout.label.clone()];
    for a in disk::btrfs_mkfs_profile_args(cfg.layout.is_raid1()) {
        args.push(a.to_string());
    }
    if cfg.layout.is_raid1() {
        println!("    mkfs.btrfs RAID1 on {}", mappers.join(" + "));
    }
    for m in &mappers {
        args.push(m.clone());
    }
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd::run("filesystems", "mkfs.btrfs", &str_args)?;
    Ok(())
}

fn btrfs_subvols(cfg: &Config) -> Result<()> {
    // Mount whole FS (any device of the multi-device FS works)
    let mapper = cfg.layout.primary().mapper_path();
    let mapper_s = mapper.display().to_string();
    cmd::run("subvolumes", "mount", &[&mapper_s, "/mnt"])?;
    for sv in disk::subvolume_names(cfg.variant.is_server()) {
        cmd::run(
            "subvolumes",
            "btrfs",
            &["subvolume", "create", &format!("/mnt/{sv}")],
        )?;
    }
    cmd::run("subvolumes", "umount", &["/mnt"])?;

    fs::create_dir_all(&cfg.mnt)?;
    let mnt = cfg.mnt.to_string_lossy();
    let opts_base = "compress=zstd:3,noatime,ssd";
    cmd::run(
        "subvolumes",
        "mount",
        &["-o", &format!("subvol=@,{opts_base}"), &mapper_s, &mnt],
    )?;

    let mounts: Vec<(&str, &str)> = if cfg.variant.is_server() {
        vec![
            ("@home", "home"),
            ("@var", "var"),
            ("@log", "var/log"),
            ("@cache", "var/cache"),
            ("@snapshots", ".snapshots"),
            ("@srv", "srv"),
        ]
    } else {
        vec![
            ("@home", "home"),
            ("@log", "var/log"),
            ("@cache", "var/cache"),
            ("@snapshots", ".snapshots"),
        ]
    };
    for (sv, dir) in mounts {
        fs::create_dir_all(cfg.mnt.join(dir))?;
        // @var then @log under var/log: create var first
        cmd::run(
            "subvolumes",
            "mount",
            &[
                "-o",
                &format!("subvol={sv},{opts_base}"),
                &mapper_s,
                &format!("{mnt}/{dir}"),
            ],
        )?;
    }
    fs::create_dir_all(cfg.mnt.join("boot"))?;
    cmd::run(
        "subvolumes",
        "mount",
        &[
            &cfg.layout.primary().efi_part.to_string_lossy(),
            &format!("{mnt}/boot"),
        ],
    )?;
    Ok(())
}

/// Copy primary ESP → secondary after boot files exist (called after first bootctl via re-run).
/// Early call after subvols only has empty EFI; full sync in install_bootloader.
fn mirror_esp(cfg: &Config) -> Result<()> {
    if cfg.layout.members.len() < 2 {
        return Ok(());
    }
    println!("    secondary ESP formatted; full boot mirror after bootctl");
    Ok(())
}

fn sync_esp_mirror(cfg: &Config) -> Result<()> {
    if cfg.layout.members.len() < 2 {
        return Ok(());
    }
    let sec = &cfg.layout.members[1].efi_part;
    let mnt2 = "/mnt/appsynergy-esp2";
    fs::create_dir_all(mnt2)?;
    cmd::run("esp-mirror", "mount", &[&sec.to_string_lossy(), mnt2])?;
    let src = cfg.mnt.join("boot");
    // rsync boot contents to second ESP
    if cmd::which("rsync") {
        cmd::run(
            "esp-mirror",
            "rsync",
            &["-aH", "--delete", &format!("{}/", src.display()), &format!("{mnt2}/")],
        )?;
    } else {
        cmd::run(
            "esp-mirror",
            "cp",
            &["-a", &format!("{}/.", src.display()), mnt2],
        )?;
    }
    cmd::run("esp-mirror", "umount", &[mnt2])?;
    println!("    mirrored ESP → {}", sec.display());
    Ok(())
}

fn pacstrap_packages(cfg: &Config) -> Result<()> {
    let mut pkgs = config::package_list(&cfg.pkgs_file)?;
    if cfg.kernel == KernelMode::Repo {
        pkgs.push("linux".into());
        pkgs.push("linux-headers".into());
    }
    let _ = cmd::run("pacstrap", "appsynergy-sanitize-mirrors", &[]);
    let mnt = cfg.mnt.to_string_lossy().into_owned();
    let mut cmd_args = vec!["-K".to_string(), mnt];
    cmd_args.extend(pkgs);
    let str_args: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
    println!("    pacstrap ({} packages)", str_args.len().saturating_sub(2));
    cmd::run("pacstrap", "pacstrap", &str_args)?;
    let _ = cmd::run("pacstrap", "appsynergy-sanitize-mirrors", &[]);
    if Path::new("/etc/pacman.d/mirrorlist").is_file() {
        fs::create_dir_all(cfg.mnt.join("etc/pacman.d"))?;
        fs::copy(
            "/etc/pacman.d/mirrorlist",
            cfg.mnt.join("etc/pacman.d/mirrorlist"),
        )?;
        cmd::arch_chroot_ok(
            &cfg.mnt,
            r#"command -v appsynergy-sanitize-mirrors >/dev/null && appsynergy-sanitize-mirrors || sed -E -i '/archlinux\.gay/d; /\.gay\//d' /etc/pacman.d/mirrorlist"#,
        );
    }
    Ok(())
}

fn install_local_kernel(cfg: &Config) -> Result<()> {
    if cfg.kernel != KernelMode::Local {
        return Ok(());
    }
    let dir = &cfg.local_pkgdir;
    let pairs = find_kernel_pkg_pairs(dir, cfg.variant)?;
    let dest = cfg.mnt.join("root/pkgs");
    fs::create_dir_all(&dest)?;
    for (pkg, hdr) in &pairs {
        println!(
            "    kernel pkgs: {} + {}",
            pkg.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
            hdr.file_name().and_then(|s| s.to_str()).unwrap_or("?")
        );
        fs::copy(pkg, dest.join(pkg.file_name().unwrap()))?;
        fs::copy(hdr, dest.join(hdr.file_name().unwrap()))?;
    }
    cmd::arch_chroot(
        &cfg.mnt,
        "pacman -U --noconfirm /root/pkgs/*.pkg.tar.zst",
    )?;
    Ok(())
}

/// Server ships **both** host-max kernels (ovh + nuc). Installer boots the match by CPU.
/// Desktop still installs a single desktop kernel package pair.
fn find_kernel_pkg_pairs(dir: &Path, variant: Variant) -> Result<Vec<(PathBuf, PathBuf)>> {
    let try_pair = |prefix: &str| -> Option<(PathBuf, PathBuf)> {
        // glob_simple only supports one `*` — filter versions in match_kernel_*.
        let mut pkgs = list_glob(dir, &format!("{prefix}-*.pkg.tar.zst"));
        let mut hdrs = list_glob(dir, &format!("{prefix}-headers-*.pkg.tar.zst"));
        pkgs.retain(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            !n.contains("-dbg-")
                && !n.contains("-headers-")
                && match_kernel_pkg_prefix(prefix, n)
        });
        hdrs.retain(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            !n.contains("-dbg-") && match_kernel_hdr_prefix(prefix, n)
        });
        pkgs.sort();
        hdrs.sort();
        if !pkgs.is_empty() && !hdrs.is_empty() {
            Some((pkgs.last().unwrap().clone(), hdrs.last().unwrap().clone()))
        } else {
            None
        }
    };

    if variant.is_server() {
        let mut pairs = Vec::new();
        // Preferred: dual max-performance host kernels
        for prefix in [
            "linux-appsynergy-server-skylake",
            "linux-appsynergy-server-tigerlake",
        ] {
            if let Some(p) = try_pair(prefix) {
                pairs.push(p);
            }
        }
        if !pairs.is_empty() {
            if pairs.len() < 2 {
                eprintln!(
                    "WARN: only {}/2 host-max server kernels present (want skylake+tigerlake)",
                    pairs.len()
                );
            }
            return Ok(pairs);
        }
        // Legacy names / portable
        for prefix in [
            "linux-appsynergy-server-ovh",
            "linux-appsynergy-server-nuc",
            "linux-appsynergy-server",
        ] {
            if let Some(p) = try_pair(prefix) {
                eprintln!("WARN: legacy kernel package prefix {prefix}; prefer skylake/tigerlake");
                pairs.push(p);
            }
        }
        if !pairs.is_empty() {
            return Ok(pairs);
        }
        eprintln!("WARN: no server kernel pkgs; falling back to linux-appsynergy");
        if let Some(p) = try_pair("linux-appsynergy").or_else(|| try_pair("linux-cachyos-igpu")) {
            return Ok(vec![p]);
        }
        bail!(
            "kernel mode local but missing pkgs in {} (variant=server; need linux-appsynergy-server-skylake + -tigerlake + headers)",
            dir.display()
        );
    }

    try_pair("linux-appsynergy")
        .or_else(|| try_pair("linux-cachyos-igpu"))
        .map(|p| vec![p])
        .with_context(|| {
            format!(
                "kernel mode local but missing pkgs in {} (variant=desktop; need linux-appsynergy + headers)",
                dir.display()
            )
        })
}

/// Which host-max server kernel flavor matches this live CPU (install-time).
/// `skylake` = Xeon E3-1270 v6 class; `tigerlake` = 11th-gen (1185G7).
fn detect_server_kernel_flavor() -> &'static str {
    let cpu = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = cpu
        .lines()
        .find(|l| l.starts_with("model name"))
        .unwrap_or("")
        .to_ascii_lowercase();
    if model.contains("e3-1270")
        || model.contains("e3-12")
        || model.contains("skylake")
        || (model.contains("xeon") && model.contains("v6"))
    {
        return "skylake";
    }
    if model.contains("1185g7")
        || model.contains("tiger lake")
        || model.contains("tigerlake")
        || model.contains("11th gen")
    {
        return "tigerlake";
    }
    eprintln!(
        "WARN: CPU not recognized for server kernel default ({model}); defaulting to skylake entry"
    );
    "skylake"
}

fn match_kernel_pkg_prefix(prefix: &str, name: &str) -> bool {
    // linux-appsynergy-7.1.3-2-x86_64.pkg.tar.zst
    // must NOT match linux-appsynergy-server-... when prefix is linux-appsynergy
    let rest = match name.strip_prefix(prefix) {
        Some(r) => r,
        None => return false,
    };
    if !rest.starts_with('-') {
        return false;
    }
    let after = &rest[1..];
    after.starts_with(|c: char| c.is_ascii_digit())
}

fn match_kernel_hdr_prefix(prefix: &str, name: &str) -> bool {
    // linux-appsynergy-headers-7.1.3-2-...
    let want = format!("{prefix}-headers-");
    name.starts_with(&want)
        && name
            .get(want.len()..)
            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
}

fn list_glob(dir: &Path, pattern: &str) -> Vec<PathBuf> {
    // simple prefix/suffix match for our patterns like name-[0-9]*.pkg.tar.zst
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut out = Vec::new();
    for ent in entries.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        if glob_simple(pattern, &name) {
            out.push(ent.path());
        }
    }
    out.sort();
    out
}

fn glob_simple(pat: &str, name: &str) -> bool {
    // only supports a single * 
    if let Some((pre, post)) = pat.split_once('*') {
        name.starts_with(pre) && name.ends_with(post) && name.len() >= pre.len() + post.len()
    } else {
        name == pat
    }
}

fn register_appsynergy_repo(cfg: &Config) -> Result<()> {
    let pacman_conf = cfg.mnt.join("etc/pacman.conf");
    let text = fs::read_to_string(&pacman_conf).context("read target pacman.conf")?;
    let mut new = text.clone();
    if !new.lines().any(|l| l.starts_with("XferCommand")) {
        new = new.replacen(
            "[options]",
            "[options]\nXferCommand = /usr/bin/curl -L -C - -f --connect-timeout 30 --retry 3 -o %o %u",
            1,
        );
    }
    if !new.contains("[appsynergy]") {
        new.push_str(&format!(
            "\n[appsynergy]\nSigLevel = Optional TrustAll\nServer = {APPSYNERGY_REPO}\n"
        ));
    }
    fs::write(&pacman_conf, new)?;
    // Do NOT pre-copy package-owned files (mirrorlist conf) — that caused
    // "exists in filesystem" on pacman -U (INSTALL-PROBLEMS #2).
    Ok(())
}

fn install_branding(cfg: &Config) -> Result<()> {
    // Try public repo first (best-effort).
    let _ = cmd::arch_chroot(
        &cfg.mnt,
        "pacman -Sy --noconfirm appsynergy-mirrorlist appsynergy-branding",
    );
    // Local packages with --overwrite for any leftover paths.
    let brands = list_glob(&cfg.local_pkgdir, "appsynergy-branding-*.pkg.tar.zst");
    let mirrors = list_glob(&cfg.local_pkgdir, "appsynergy-mirrorlist-*.pkg.tar.zst");
    let local: Vec<PathBuf> = brands.into_iter().chain(mirrors).collect();
    if !local.is_empty() {
        let dest = cfg.mnt.join("root/pkgs");
        fs::create_dir_all(&dest)?;
        for p in &local {
            fs::copy(p, dest.join(p.file_name().unwrap()))?;
        }
        cmd::arch_chroot(
            &cfg.mnt,
            r#"
set -e
shopt -s nullglob
pkgs=(/root/pkgs/appsynergy-branding-*.pkg.tar.zst /root/pkgs/appsynergy-mirrorlist-*.pkg.tar.zst)
if ((${#pkgs[@]})); then
  pacman -U --noconfirm --overwrite '*' "${pkgs[@]}"
fi
"#,
        )?;
        println!("    branding/mirrorlist installed (local, --overwrite)");
    } else {
        eprintln!("WARN: no local branding pkgs; repo path may have failed");
    }
    let skel = cfg.mnt.join("usr/share/appsynergy/skel/bashrc");
    if skel.is_file() {
        fs::copy(&skel, cfg.mnt.join("etc/skel/.bashrc"))?;
        println!("    applied /etc/skel/.bashrc");
    }
    if !cfg.mnt.join("etc/motd").is_file() {
        eprintln!("WARN: /etc/motd missing after branding install");
    }
    Ok(())
}

fn install_browsers(cfg: &Config) -> Result<()> {
    if cfg.variant.is_server() {
        println!("    skip browsers (server variant)");
        return Ok(());
    }
    let mut pkgs = list_glob(&cfg.local_pkgdir, "brave-bin-*.pkg.tar.zst");
    pkgs.extend(list_glob(&cfg.local_pkgdir, "thorium-browser-bin-*.pkg.tar.zst"));
    if pkgs.is_empty() {
        eprintln!("    WARN: no brave-bin / thorium pkgs — install after boot");
        return Ok(());
    }
    let dest = cfg.mnt.join("root/pkgs");
    fs::create_dir_all(&dest)?;
    for p in &pkgs {
        fs::copy(p, dest.join(p.file_name().unwrap()))?;
    }
    cmd::arch_chroot(
        &cfg.mnt,
        r#"
shopt -s nullglob
pkgs=(/root/pkgs/brave-bin-*.pkg.tar.zst /root/pkgs/thorium-browser-bin-*.pkg.tar.zst)
if ((${#pkgs[@]})); then pacman -U --noconfirm --overwrite '*' "${pkgs[@]}"; fi
"#,
    )?;
    Ok(())
}

fn fstab_crypttab(cfg: &Config) -> Result<()> {
    let mnt = cfg.mnt.to_string_lossy();
    let out = cmd::output("fstab", "genfstab", &["-U", &mnt])?;
    let fstab = cfg.mnt.join("etc/fstab");
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&fstab)
        .and_then(|mut f| write!(f, "{out}\n"))
        .context("write fstab")?;

    let mut crypt_entries = Vec::new();
    let mut uuid_lines = String::new();
    let mut dev_lines = String::new();
    for m in &cfg.layout.members {
        let uuid = cmd::output(
            "fstab",
            "blkid",
            &["-s", "UUID", "-o", "value", &m.luks_part.to_string_lossy()],
        )?;
        if uuid.is_empty() {
            bail!("empty LUKS UUID for {}", m.luks_part.display());
        }
        crypt_entries.push((m.cryptname.clone(), uuid.clone(), false));
        uuid_lines.push_str(&format!("{} {}\n", m.cryptname, uuid));
        dev_lines.push_str(&format!("{} {}\n", m.cryptname, m.luks_part.display()));
    }
    // TPM flag filled later in enroll; initial crypttab without tpm
    let crypttab = disk::render_crypttab(
        &crypt_entries
            .iter()
            .map(|(n, u, _)| (n.clone(), u.clone(), false))
            .collect::<Vec<_>>(),
    );
    fs::write(cfg.mnt.join("etc/crypttab"), &crypttab)?;
    let _ = fs::write(cfg.mnt.join("etc/crypttab.initramfs"), &crypttab);
    fs::create_dir_all(cfg.mnt.join("etc/appsynergy"))?;
    // primary uuid line for boot entry (first); full map for dual
    fs::write(
        cfg.mnt.join("etc/appsynergy/luks-uuid"),
        format!("{}\n", crypt_entries[0].1),
    )?;
    fs::write(cfg.mnt.join("etc/appsynergy/luks-uuids"), uuid_lines)?;
    fs::write(cfg.mnt.join("etc/appsynergy/luks-devices"), dev_lines)?;
    fs::write(
        cfg.mnt.join("etc/appsynergy/luks-device"),
        format!("{}\n", cfg.layout.primary().luks_part.display()),
    )?;
    if cfg.layout.is_raid1() {
        fs::write(cfg.mnt.join("etc/appsynergy/btrfs-raid1"), "1\n")?;
    }
    // btrfs UUID for root=
    let btrfs_uuid = cmd::output(
        "fstab",
        "blkid",
        &[
            "-s",
            "UUID",
            "-o",
            "value",
            &cfg.layout.primary().mapper_path().to_string_lossy(),
        ],
    )?;
    if !btrfs_uuid.is_empty() {
        fs::write(
            cfg.mnt.join("etc/appsynergy/btrfs-uuid"),
            format!("{btrfs_uuid}\n"),
        )?;
    }
    Ok(())
}

fn locale_hostname(cfg: &Config) -> Result<()> {
    // Write vconsole.conf *before* any mkinitcpio (INSTALL-PROBLEMS #4)
    fs::write(
        cfg.mnt.join("etc/vconsole.conf"),
        format!("KEYMAP={}\n", cfg.keymap),
    )?;
    fs::write(
        cfg.mnt.join("etc/locale.conf"),
        format!("LANG={}\n", cfg.locale),
    )?;
    fs::write(cfg.mnt.join("etc/hostname"), format!("{}\n", cfg.hostname))?;
    fs::write(
        cfg.mnt.join("etc/hosts"),
        format!(
            "127.0.0.1   localhost\n::1         localhost\n127.0.1.1   {}.localdomain {}\n",
            cfg.hostname, cfg.hostname
        ),
    )?;

    let script = format!(
        r#"
set -euo pipefail
ln -sf /usr/share/zoneinfo/{tz} /etc/localtime
hwclock --systohc || true
if grep -q '^{loc}' /etc/locale.gen 2>/dev/null; then
  sed -i 's/^#{loc}/{loc}/' /etc/locale.gen || true
else
  sed -i 's/^#{loc}/{loc}/' /etc/locale.gen 2>/dev/null || echo '{loc} UTF-8' >> /etc/locale.gen
fi
# ensure uncommented
grep -q '^{loc}' /etc/locale.gen || echo '{loc} UTF-8' >> /etc/locale.gen
locale-gen
"#,
        tz = cfg.timezone,
        loc = cfg.locale,
    );
    cmd::arch_chroot(&cfg.mnt, &script)?;
    Ok(())
}

/// INSTALL-PROBLEMS #1: never cp through the /etc/os-release → usr/lib symlink.
fn apply_os_release(cfg: &Config) -> Result<()> {
    let src_candidates = [
        cfg.mnt.join("etc/appsynergy/os-release"),
        cfg.mnt.join("usr/share/appsynergy/os-release"),
    ];
    let src = src_candidates.into_iter().find(|p| p.is_file());
    // disk::os_release_write_plan() — only write lib, then symlink etc
    let (lib_rel, link_tgt) = disk::os_release_write_plan();
    let lib = cfg.mnt.join(lib_rel);
    let etc = cfg.mnt.join("etc/os-release");

    if let Some(src) = src {
        // Always write to real file path, never through /etc symlink
        fs::copy(&src, &lib).with_context(|| format!("copy {} -> {}", src.display(), lib.display()))?;
        if cfg.variant.is_server() {
            // Rebrand PRETTY_NAME for server without a separate package.
            if let Ok(mut t) = fs::read_to_string(&lib) {
                t = t.replace("AppSynergy Linux", "AppSynergy Server");
                if !t.contains("VARIANT=") {
                    t.push_str("VARIANT=\"Server\"\nVARIANT_ID=server\n");
                }
                fs::write(&lib, t)?;
            }
        }
    } else {
        let body = if cfg.variant.is_server() {
            r#"NAME="AppSynergy Server"
PRETTY_NAME="AppSynergy Server"
ID=appsynergy-linux
ID_LIKE=arch
VARIANT="Server"
VARIANT_ID=server
BUILD_ID=rolling
ANSI_COLOR="0;36"
HOME_URL="https://git.appsynergy.io/imabee"
DOCUMENTATION_URL="https://git.appsynergy.io/imabee"
SUPPORT_URL="https://git.appsynergy.io/imabee"
LOGO=appsynergy-linux
"#
        } else {
            r#"NAME="AppSynergy Linux"
PRETTY_NAME="AppSynergy Linux"
ID=appsynergy-linux
ID_LIKE=arch
BUILD_ID=rolling
ANSI_COLOR="0;36"
HOME_URL="https://git.appsynergy.io/imabee"
DOCUMENTATION_URL="https://git.appsynergy.io/imabee"
SUPPORT_URL="https://git.appsynergy.io/imabee"
LOGO=appsynergy-linux
"#
        };
        fs::write(&lib, body)?;
    }
    // Replace /etc/os-release with a relative symlink (Arch convention).
    if etc.exists() || etc.symlink_metadata().is_ok() {
        fs::remove_file(&etc).ok();
    }
    std::os::unix::fs::symlink(link_tgt, &etc)
        .with_context(|| format!("symlink {}", etc.display()))?;
    println!("    wrote /{lib_rel}; /etc/os-release -> {link_tgt}");
    Ok(())
}

fn network_setup(cfg: &Config) -> Result<()> {
    if cfg.variant.is_server() {
        // systemd-networkd DHCP on en*/eth*; no NetworkManager / bluetooth.
        let netdir = cfg.mnt.join("etc/systemd/network");
        fs::create_dir_all(&netdir)?;
        let live = Path::new("/etc/appsynergy/server-network/20-wired.network");
        if live.is_file() {
            fs::copy(live, netdir.join("20-wired.network"))?;
        } else {
            fs::write(
                netdir.join("20-wired.network"),
                "[Match]\nName=en* eth*\n\n[Network]\nDHCP=yes\nIPv6AcceptRA=yes\n",
            )?;
        }
        // resolved: use stub
        let resolved = cfg.mnt.join("etc/systemd/resolved.conf.d");
        fs::create_dir_all(&resolved)?;
        fs::write(
            resolved.join("appsynergy.conf"),
            "[Resolve]\nDNS=1.1.1.1 9.9.9.9\nFallbackDNS=8.8.8.8\n",
        )?;
        println!("    server network: systemd-networkd + resolved (DHCP en*/eth*)");
        return Ok(());
    }

    let nm = cfg.mnt.join("etc/NetworkManager/conf.d");
    fs::create_dir_all(&nm)?;
    let wifi = Path::new("/etc/NetworkManager/conf.d/wifi_backend.conf");
    if wifi.is_file() {
        fs::copy(wifi, nm.join("wifi_backend.conf"))?;
    } else {
        fs::write(nm.join("wifi_backend.conf"), "[device]\nwifi.backend=iwd\n")?;
    }

    let bt_dir = cfg.mnt.join("etc/bluetooth");
    fs::create_dir_all(&bt_dir)?;
    let main_conf = bt_dir.join("main.conf");
    if main_conf.is_file() {
        let mut t = fs::read_to_string(&main_conf)?;
        t = set_ini_key(&t, "Experimental", "true");
        t = set_ini_key(&t, "AutoEnable", "true");
        fs::write(&main_conf, t)?;
    } else {
        fs::write(
            &main_conf,
            "[General]\nExperimental = true\n[Policy]\nAutoEnable = true\n",
        )?;
    }
    Ok(())
}

fn set_ini_key(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let mut found = false;
    for line in &mut lines {
        let trim = line.trim();
        if trim.starts_with('#') {
            let rest = trim.trim_start_matches('#').trim();
            if rest.starts_with(key) && rest.contains('=') {
                *line = format!("{key} = {value}");
                found = true;
            }
        } else if trim.starts_with(key) && trim.contains('=') {
            *line = format!("{key} = {value}");
            found = true;
        }
    }
    if !found {
        lines.push(format!("{key} = {value}"));
    }
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn configure_mkinitcpio(cfg: &Config) -> Result<()> {
    let path = cfg.mnt.join("etc/mkinitcpio.conf");
    let mut text = fs::read_to_string(&path).unwrap_or_default();
    // Server: netconf + dropbear before sd-encrypt so SSH unlock works when TPM fails.
    // appsynergy-ssh-unlock sets root shell to the passphrase agent.
    let hooks = if cfg.variant.is_server() && cfg.ssh_pubkey.is_some() {
        "HOOKS=(base systemd autodetect microcode modconf kms keyboard sd-vconsole block netconf dropbear appsynergy-ssh-unlock sd-encrypt filesystems fsck)"
    } else if cfg.variant.is_server() {
        "HOOKS=(base systemd autodetect microcode modconf kms keyboard sd-vconsole block sd-encrypt filesystems fsck)"
    } else {
        "HOOKS=(base systemd autodetect microcode modconf kms keyboard sd-vconsole block sd-encrypt filesystems fsck)"
    };
    if text.lines().any(|l| l.starts_with("HOOKS=")) {
        let mut out = String::new();
        for line in text.lines() {
            if line.starts_with("HOOKS=") {
                out.push_str(hooks);
            } else {
                out.push_str(line);
            }
            out.push('\n');
        }
        text = out;
    } else {
        text.push_str(hooks);
        text.push('\n');
    }
    // Desktop: Keychron Mac-mode needs hid_apple in initramfs for LUKS.
    // Server: OVH IPMI USB HID is generic; skip special driver.
    if !cfg.variant.is_server() && !text.contains("hid_apple") {
        if text.contains("MODULES=()") {
            text = text.replace("MODULES=()", "MODULES=(hid_apple)");
        } else if let Some(start) = text.find("MODULES=(") {
            if let Some(end) = text[start..].find(')') {
                let end = start + end;
                text.insert_str(end, " hid_apple");
            }
        } else {
            text.push_str("MODULES=(hid_apple)\n");
        }
    }
    fs::write(&path, text)?;
    // Ensure vconsole exists (already written in locale step)
    if !cfg.mnt.join("etc/vconsole.conf").is_file() {
        fs::write(
            cfg.mnt.join("etc/vconsole.conf"),
            format!("KEYMAP={}\n", cfg.keymap),
        )?;
    }
    Ok(())
}

/// Install existing operator pubkey (from live `/etc/appsynergy/ssh-unlock.pub` or override)
/// onto target root + admin user. Does not generate keys.
fn install_ssh_keys(cfg: &Config) -> Result<()> {
    let Some(ref key) = cfg.ssh_pubkey else {
        eprintln!("WARN: no SSH pubkey on live image (/etc/appsynergy/ssh-unlock.pub) — skip authorized_keys");
        return Ok(());
    };
    fs::create_dir_all(cfg.mnt.join("root/.ssh"))?;
    let root_auth = cfg.mnt.join("root/.ssh/authorized_keys");
    fs::write(&root_auth, key)?;
    let _ = cmd::run(
        "ssh-keys",
        "chmod",
        &["700", &cfg.mnt.join("root/.ssh").to_string_lossy()],
    );
    let _ = cmd::run("ssh-keys", "chmod", &["600", &root_auth.to_string_lossy()]);

    let user_home = cfg.mnt.join("home").join(&cfg.user);
    if user_home.is_dir() {
        let user_ssh = user_home.join(".ssh");
        fs::create_dir_all(&user_ssh)?;
        fs::write(user_ssh.join("authorized_keys"), key)?;
        cmd::arch_chroot_ok(
            &cfg.mnt,
            &format!(
                "chown -R {u}:{u} /home/{u}/.ssh && chmod 700 /home/{u}/.ssh && chmod 600 /home/{u}/.ssh/authorized_keys",
                u = cfg.user
            ),
        );
    }
    println!(
        "    SSH pubkey → root + {} (baked; no new key generated)",
        cfg.user
    );
    Ok(())
}

fn create_users(cfg: &Config) -> Result<()> {
    let groups = if cfg.variant.is_server() {
        "wheel,systemd-journal,tss"
    } else {
        // no docker group — containerd/nerdctl (rootful via sudo, or rootless later)
        "wheel,audio,input,video,lp,rfkill,storage,network,tss,uucp"
    };
    let pam_wallet = if cfg.variant.is_server() {
        ""
    } else {
        r#"
for pamf in /etc/pam.d/sddm /etc/pam.d/sddm-autologin; do
  [[ -f $pamf ]] || continue
  if ! grep -q pam_kwallet5.so $pamf; then
    echo '-auth       optional    pam_kwallet5.so' >> $pamf
    echo '-session    optional    pam_kwallet5.so auto_start' >> $pamf
  fi
done
"#
    };
    if let Some(ref pw) = cfg.password {
        let root_line = format!("root:{}\n", String::from_utf8_lossy(pw));
        let user_line = format!("{}:{}\n", cfg.user, String::from_utf8_lossy(pw));
        let script = format!(
            r#"
set -euo pipefail
id '{user}' >/dev/null 2>&1 || useradd -m -G {groups} -s /bin/bash '{user}'
usermod -aG {groups} '{user}' || true
mkdir -p /etc/sudoers.d
echo '%wheel ALL=(ALL:ALL) ALL' > /etc/sudoers.d/wheel
chmod 440 /etc/sudoers.d/wheel
{pam}
"#,
            user = cfg.user,
            groups = groups,
            pam = pam_wallet,
        );
        cmd::arch_chroot(&cfg.mnt, &script)?;
        let pwfile = cfg.mnt.join("root/.appsynergy-chpasswd");
        let mut body = root_line;
        body.push_str(&user_line);
        fs::write(&pwfile, body.as_bytes())?;
        let _ = cmd::run(
            "users",
            "chmod",
            &["600", &pwfile.to_string_lossy()],
        );
        cmd::arch_chroot(
            &cfg.mnt,
            "chpasswd < /root/.appsynergy-chpasswd && rm -f /root/.appsynergy-chpasswd",
        )?;
        println!("    root + {} passwords set from keyfile", cfg.user);
    } else {
        let script = format!(
            r#"
set -euo pipefail
echo 'Root password:'
passwd
useradd -m -G {groups} -s /bin/bash '{user}' 2>/dev/null || usermod -aG {groups} '{user}' || true
echo 'Password for {user}:'
passwd '{user}'
mkdir -p /etc/sudoers.d
echo '%wheel ALL=(ALL:ALL) ALL' > /etc/sudoers.d/wheel
chmod 440 /etc/sudoers.d/wheel
{pam}
"#,
            user = cfg.user,
            groups = groups,
            pam = pam_wallet,
        );
        cmd::arch_chroot(&cfg.mnt, &script)?;
    }
    Ok(())
}

fn install_bootloader(cfg: &Config) -> Result<()> {
    cmd::arch_chroot(&cfg.mnt, "bootctl install")?;
    // List all installed kernels (server ships ovh + nuc).
    let vmlinuz_list = cmd::output(
        "bootloader",
        "arch-chroot",
        &[
            &cfg.mnt.to_string_lossy(),
            "bash",
            "-c",
            "ls -1 /boot/vmlinuz-* 2>/dev/null | xargs -n1 basename",
        ],
    )?;
    let kernels: Vec<String> = vmlinuz_list
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if kernels.is_empty() {
        bail!("could not find kernel images in /boot");
    }
    // LUKS uuid map: cryptname uuid per line
    let mut luks_pairs: Vec<(String, String)> = Vec::new();
    if let Ok(map) = fs::read_to_string(cfg.mnt.join("etc/appsynergy/luks-uuids")) {
        for line in map.lines() {
            let mut sp = line.split_whitespace();
            if let (Some(n), Some(u)) = (sp.next(), sp.next()) {
                luks_pairs.push((n.to_string(), u.to_string()));
            }
        }
    }
    if luks_pairs.is_empty() {
        let u = fs::read_to_string(cfg.mnt.join("etc/appsynergy/luks-uuid"))?
            .trim()
            .to_string();
        luks_pairs.push((cfg.cryptname.clone(), u));
    }
    let root_spec = if let Ok(bu) = fs::read_to_string(cfg.mnt.join("etc/appsynergy/btrfs-uuid")) {
        let bu = bu.trim();
        if bu.is_empty() {
            format!("/dev/mapper/{}", cfg.layout.primary().cryptname)
        } else {
            format!("UUID={bu}")
        }
    } else {
        format!("/dev/mapper/{}", cfg.layout.primary().cryptname)
    };
    let mut extra = String::new();
    if cfg.variant.is_server() {
        extra.push_str("preempt=voluntary");
        if cfg.ssh_pubkey.is_some() {
            extra.push_str(" ip=dhcp");
        }
    } else {
        extra.push_str("nowatchdog");
    }
    let options = disk::render_cmdline_luks(&luks_pairs, &root_spec, &extra);

    fs::create_dir_all(cfg.mnt.join("boot/loader/entries"))?;
    let timeout = if cfg.variant.is_server() { 2 } else { 3 };
    // Prefer matching ucode if present on ESP after pacstrap.
    let ucode = if cfg.mnt.join("boot/amd-ucode.img").is_file()
        && !cfg.mnt.join("boot/intel-ucode.img").is_file()
    {
        "amd-ucode.img"
    } else {
        "intel-ucode.img"
    };

    // Map vmlinuz-* → flavor for entry name/title (server dual kernels).
    let preferred = if cfg.variant.is_server() {
        detect_server_kernel_flavor()
    } else {
        ""
    };
    let mut default_entry = String::from("appsynergy.conf");
    let mut wrote_any = false;

    for vmlinuz in &kernels {
        // vmlinuz-…-appsynergy-server-skylake → initramfs-…-appsynergy-server-skylake.img
        let kver = vmlinuz.strip_prefix("vmlinuz-").unwrap_or(vmlinuz);
        let initramfs = format!("initramfs-{kver}.img");
        if !cfg.mnt.join("boot").join(&initramfs).is_file() {
            eprintln!("WARN: missing {initramfs} for {vmlinuz}; skipping entry");
            continue;
        }

        let (entry_name, title) = if kver.contains("appsynergy-server-skylake")
            || kver.contains("appsynergy-server-ovh")
        {
            ("appsynergy-skylake.conf", "AppSynergy Server (skylake)")
        } else if kver.contains("appsynergy-server-tigerlake")
            || kver.contains("appsynergy-server-nuc")
        {
            ("appsynergy-tigerlake.conf", "AppSynergy Server (tigerlake)")
        } else if kver.contains("appsynergy-server") {
            ("appsynergy.conf", cfg.variant.boot_entry_title())
        } else if cfg.variant.is_server() {
            ("appsynergy.conf", cfg.variant.boot_entry_title())
        } else {
            ("appsynergy.conf", cfg.variant.boot_entry_title())
        };

        if cfg.variant.is_server() {
            if preferred == "skylake" && entry_name == "appsynergy-skylake.conf" {
                default_entry = entry_name.to_string();
            } else if preferred == "tigerlake" && entry_name == "appsynergy-tigerlake.conf" {
                default_entry = entry_name.to_string();
            }
        } else {
            default_entry = entry_name.to_string();
        }

        let mut entry = format!("title   {title}\nlinux   /{vmlinuz}\n");
        if cfg.mnt.join("boot").join(ucode).is_file() {
            entry.push_str(&format!("initrd  /{ucode}\n"));
        }
        if ucode == "intel-ucode.img" && cfg.mnt.join("boot/amd-ucode.img").is_file() {
            entry.push_str("initrd  /amd-ucode.img\n");
        }
        entry.push_str(&format!("initrd  /{initramfs}\noptions {options}\n"));
        fs::write(
            cfg.mnt.join("boot/loader/entries").join(entry_name),
            entry,
        )?;
        wrote_any = true;
        println!("    boot entry: {entry_name} → {vmlinuz}");
    }

    if !wrote_any {
        bail!("no boot entries written (missing initramfs for installed kernels)");
    }

    // If preferred flavor package missing, default to first host-max entry present.
    if cfg.variant.is_server() {
        let entries_dir = cfg.mnt.join("boot/loader/entries");
        if !entries_dir.join(&default_entry).is_file() {
            for cand in [
                "appsynergy-skylake.conf",
                "appsynergy-tigerlake.conf",
                "appsynergy.conf",
            ] {
                if entries_dir.join(cand).is_file() {
                    default_entry = cand.to_string();
                    break;
                }
            }
        }
        println!(
            "    server kernel default: {default_entry} (detected flavor={preferred})"
        );
    }

    fs::write(
        cfg.mnt.join("boot/loader/loader.conf"),
        format!("default {default_entry}\ntimeout {timeout}\nconsole-mode keep\n"),
    )?;
    // Dual disk: mirror ESP for failover boot
    if cfg.layout.is_raid1() {
        sync_esp_mirror(cfg)?;
    }
    Ok(())
}

/// INSTALL-PROBLEMS: bootctl install may not fix NVRAM after a full wipe.
fn fix_efi_nvram(cfg: &Config) -> Result<()> {
    // PARTUUID of new ESP
    let partuuid = cmd::output(
        "efibootmgr",
        "blkid",
        &[
            "-s",
            "PARTUUID",
            "-o",
            "value",
            &cfg.efi_part.to_string_lossy(),
        ],
    )?;
    if partuuid.is_empty() {
        eprintln!("WARN: could not read ESP PARTUUID; skip NVRAM fix");
        return Ok(());
    }
    println!("    ESP PARTUUID={partuuid}");

    // Create / refresh AppSynergy entry for each ESP (dual NVMe failover).

    // Remove existing AppSynergy entries first to avoid duplicates
    if let Ok(list) = cmd::output("efibootmgr", "efibootmgr", &["-v"]) {
        for line in list.lines() {
            if line.contains("AppSynergy Linux") || line.contains("AppSynergy Server") {
                if let Some(boot) = line.split_whitespace().next() {
                    let num = boot.trim_start_matches("Boot").trim_end_matches('*');
                    if num.chars().all(|c| c.is_ascii_hexdigit()) {
                        let _ = cmd::run(
                            "efibootmgr",
                            "efibootmgr",
                            &["-b", num, "-B"],
                        );
                    }
                }
            }
        }
        // Drop entries whose PARTUUID is not our new ESP (stale Windows/Linux)
        for line in list.lines() {
            if !line.contains("HD(") {
                continue;
            }
            if line.contains(&partuuid) {
                continue;
            }
            // stale entry referencing another disk/path
            if line.contains("Windows Boot Manager")
                || line.contains("Linux Boot Manager")
                || line.contains("systemd-boot")
                || line.contains("SYSTEMD-BOOT")
            {
                if let Some(boot) = line.split_whitespace().next() {
                    let num = boot.trim_start_matches("Boot").trim_end_matches('*');
                    if num.chars().all(|c| c.is_ascii_hexdigit()) {
                        println!("    removing stale NVRAM entry {boot}");
                        let _ = cmd::run(
                            "efibootmgr",
                            "efibootmgr",
                            &["-b", num, "-B"],
                        );
                    }
                }
            }
        }
    }

    let label = cfg.variant.boot_entry_title();
    for (i, m) in cfg.layout.members.iter().enumerate() {
        let disk = m.disk.to_string_lossy();
        let part_num = disk::efi_part_number(&m.efi_part);
        let lab = if i == 0 {
            label.to_string()
        } else {
            format!("{label} (disk{i})")
        };
        cmd::run(
            "efibootmgr",
            "efibootmgr",
            &[
                "-c",
                "-d",
                &disk,
                "-p",
                &part_num.to_string(),
                "-L",
                &lab,
                "-l",
                r"\EFI\systemd\systemd-bootx64.efi",
            ],
        )?;
        println!("    NVRAM entry {lab} -> {}", m.efi_part.display());
    }
    let _ = partuuid; // primary ESP uuid already logged
    Ok(())
}

fn rebuild_initramfs(cfg: &Config) -> Result<()> {
    // vconsole already present
    cmd::arch_chroot(&cfg.mnt, "mkinitcpio -P")?;
    Ok(())
}

/// Enroll TPM2 unlock for the LUKS volume (passphrase slot kept).
/// Runs from the live host against the real block device; then rewrites
/// target crypttab and rebuilds initramfs in the chroot.
fn enroll_tpm(cfg: &Config) -> Result<()> {
    if !cfg.tpm {
        println!("    skip (TPM enrollment disabled)");
        return Ok(());
    }
    if !cmd::which("systemd-cryptenroll") {
        if cfg.tpm_required {
            bail!("systemd-cryptenroll not found");
        }
        eprintln!("WARN: systemd-cryptenroll missing — skip TPM");
        return Ok(());
    }
    let tpm_ok = Path::new("/dev/tpm0").exists() || Path::new("/dev/tpmrm0").exists();
    if !tpm_ok {
        if cfg.tpm_required {
            bail!("no TPM device (/dev/tpm0 or /dev/tpmrm0)");
        }
        eprintln!("WARN: no TPM device — skip TPM enrollment");
        return Ok(());
    }

    println!("    PCRS: {}", cfg.tpm_pcrs);
    println!("    passphrase slot kept as recovery");

    let keyfile_path = PathBuf::from("/tmp/appsynergy-tpm-unlock.key");
    let key_owned: Option<PathBuf> = if let Some(ref pw) = cfg.password {
        fs::write(&keyfile_path, pw).context("write temp unlock keyfile")?;
        let _ = cmd::run("tpm-enroll", "chmod", &["600", &keyfile_path.to_string_lossy()]);
        Some(keyfile_path.clone())
    } else {
        None
    };

    let pcrs = cfg.tpm_pcrs.clone();
    let mut enroll_err: Option<anyhow::Error> = None;
    for m in &cfg.layout.members {
        let dev = m.luks_part.to_string_lossy();
        println!("    enroll TPM on {dev}");
        let r = if let Some(ref kf) = key_owned {
            cmd::run(
                "tpm-enroll",
                "systemd-cryptenroll",
                &[
                    "--tpm2-device=auto",
                    &format!("--tpm2-pcrs={pcrs}"),
                    &format!("--unlock-key-file={}", kf.display()),
                    &dev,
                ],
            )
        } else {
            println!("    type LUKS passphrase when systemd-cryptenroll asks");
            cmd::run(
                "tpm-enroll",
                "systemd-cryptenroll",
                &[
                    "--tpm2-device=auto",
                    &format!("--tpm2-pcrs={pcrs}"),
                    &dev,
                ],
            )
        };
        if let Err(e) = r {
            enroll_err = Some(e);
            break;
        }
    }

    if key_owned.is_some() {
        let _ = fs::remove_file(&keyfile_path);
    }

    if let Some(e) = enroll_err {
        if cfg.tpm_required {
            return Err(e).context("TPM enrollment failed");
        }
        eprintln!("WARN: TPM enrollment failed (continuing): {e:#}");
        eprintln!("    re-run after boot: sudo appsynergy-tpm-enroll");
        return Ok(());
    }

    // crypttab: TPM on every LUKS volume
    let mut entries = Vec::new();
    if let Ok(map) = fs::read_to_string(cfg.mnt.join("etc/appsynergy/luks-uuids")) {
        for line in map.lines() {
            let mut sp = line.split_whitespace();
            if let (Some(n), Some(u)) = (sp.next(), sp.next()) {
                entries.push((n.to_string(), u.to_string(), true));
            }
        }
    }
    if entries.is_empty() {
        let luks_uuid = fs::read_to_string(cfg.mnt.join("etc/appsynergy/luks-uuid"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if luks_uuid.is_empty() {
            bail!("missing luks-uuid after enroll");
        }
        entries.push((cfg.cryptname.clone(), luks_uuid, true));
    }
    let crypttab = disk::render_crypttab(&entries);
    fs::write(cfg.mnt.join("etc/crypttab"), &crypttab)?;
    let _ = fs::write(cfg.mnt.join("etc/crypttab.initramfs"), &crypttab);
    fs::write(cfg.mnt.join("etc/appsynergy/tpm-enrolled"), "1\n")?;
    fs::write(
        cfg.mnt.join("etc/appsynergy/tpm-pcrs"),
        format!("{}\n", cfg.tpm_pcrs),
    )?;

    println!("    rebuilding initramfs with tpm2 crypttab");
    cmd::arch_chroot(&cfg.mnt, "mkinitcpio -P")?;
    println!("    TPM2 token enrolled (PCRS={})", cfg.tpm_pcrs);
    Ok(())
}

fn enable_services(cfg: &Config) -> Result<()> {
    if cfg.variant.is_server() {
        cmd::arch_chroot_ok(
            &cfg.mnt,
            "systemctl enable sshd systemd-networkd systemd-resolved nftables apparmor containerd fstrim.timer || true",
        );
        // Point /etc/resolv.conf at resolved stub when possible
        cmd::arch_chroot_ok(
            &cfg.mnt,
            r#"ln -sfn /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf || true"#,
        );
        // Explicitly avoid desktop services / docker if packages ever leak in
        cmd::arch_chroot_ok(
            &cfg.mnt,
            "systemctl disable sddm NetworkManager bluetooth docker 2>/dev/null || true",
        );
    } else {
        cmd::arch_chroot_ok(
            &cfg.mnt,
            "systemctl enable NetworkManager sddm sshd containerd fstrim.timer bluetooth || true",
        );
        cmd::arch_chroot_ok(
            &cfg.mnt,
            "systemctl disable docker 2>/dev/null || true",
        );
        cmd::arch_chroot_ok(&cfg.mnt, "systemctl enable obex || true");
        cmd::arch_chroot_ok(
            &cfg.mnt,
            "systemctl --global enable plasma-kwallet-pam.service || true",
        );
    }
    Ok(())
}

/// Server-only: hardening + SSH unlock wiring. No-op for desktop.
/// Takes security posture from appsynergy-linux host overlay — not agents/pets/console.
fn apply_server_overlay(cfg: &Config) -> Result<()> {
    if !cfg.variant.is_server() {
        return Ok(());
    }
    fs::create_dir_all(cfg.mnt.join("etc/sysctl.d"))?;
    fs::create_dir_all(cfg.mnt.join("etc/modules-load.d"))?;
    fs::create_dir_all(cfg.mnt.join("usr/lib/appsynergy"))?;
    fs::create_dir_all(cfg.mnt.join("etc/initcpio/install"))?;
    fs::create_dir_all(cfg.mnt.join("etc/dropbear"))?;
    fs::create_dir_all(cfg.mnt.join("etc/ssh/sshd_config.d"))?;
    fs::create_dir_all(cfg.mnt.join("etc/systemd/journald.conf.d"))?;
    fs::create_dir_all(cfg.mnt.join("etc/systemd/system.conf.d"))?;
    fs::create_dir_all(cfg.mnt.join("etc/systemd/network"))?;
    fs::create_dir_all(cfg.mnt.join("root/.ssh"))?;
    fs::create_dir_all(cfg.mnt.join("usr/local/bin"))?;

    let sysctl_src = Path::new("/etc/appsynergy/sysctl-server.conf");
    let sysctl_dst = cfg.mnt.join("etc/sysctl.d/99-appsynergy-server.conf");
    if sysctl_src.is_file() {
        fs::copy(sysctl_src, &sysctl_dst)?;
    } else {
        eprintln!("WARN: missing {sysctl_src:?}; writing minimal forward sysctl");
        fs::write(
            &sysctl_dst,
            "net.ipv4.ip_forward = 1\nnet.ipv6.conf.all.forwarding = 1\nnet.core.default_qdisc = fq\nnet.ipv4.tcp_congestion_control = bbr\nkernel.kptr_restrict = 1\nkernel.dmesg_restrict = 1\nkernel.unprivileged_bpf_disabled = 1\n",
        )?;
    }

    let mod_src = Path::new("/etc/appsynergy/modules-load-server.conf");
    let mod_dst = cfg.mnt.join("etc/modules-load.d/appsynergy-server.conf");
    if mod_src.is_file() {
        fs::copy(mod_src, &mod_dst)?;
    } else {
        fs::write(&mod_dst, "nf_conntrack\nnf_tables\n")?;
    }

    let nft_src = Path::new("/etc/appsynergy/server-nftables.conf");
    let nft_dst = cfg.mnt.join("etc/nftables.conf");
    if nft_src.is_file() {
        fs::copy(nft_src, &nft_dst)?;
    }

    // journald + watchdog (appsynergy-linux host hardening, no apps)
    copy_or(
        Path::new("/etc/appsynergy/server/journald.conf"),
        &cfg.mnt.join("etc/systemd/journald.conf.d/10-appsynergy.conf"),
        "[Journal]\nStorage=persistent\nSystemMaxUse=200M\nRateLimitIntervalSec=30s\nRateLimitBurst=1000\n",
    )?;
    copy_or(
        Path::new("/etc/appsynergy/server/watchdog.conf"),
        &cfg.mnt.join("etc/systemd/system.conf.d/10-watchdog.conf"),
        "[Manager]\nRuntimeWatchdogSec=30s\nRebootWatchdogSec=2min\n",
    )?;
    copy_or(
        Path::new("/etc/appsynergy/server/sshd-hardening.conf"),
        &cfg.mnt.join("etc/ssh/sshd_config.d/10-appsynergy.conf"),
        "PermitRootLogin prohibit-password\nPasswordAuthentication no\nPubkeyAuthentication yes\nAuthenticationMethods publickey\nX11Forwarding no\n",
    )?;

    // Initrd unlock shell + mkinitcpio install hook
    let unlock_src = Path::new("/etc/appsynergy/server/initrd-unlock");
    let unlock_dst = cfg.mnt.join("usr/lib/appsynergy/initrd-unlock");
    if unlock_src.is_file() {
        fs::copy(unlock_src, &unlock_dst)?;
    } else {
        fs::write(
            &unlock_dst,
            "#!/bin/sh\nexec systemd-tty-ask-password-agent --query\n",
        )?;
    }
    let _ = cmd::run(
        "server-overlay",
        "chmod",
        &["755", &unlock_dst.to_string_lossy()],
    );

    let hook_src = Path::new("/etc/appsynergy/server/initcpio-install-ssh-unlock");
    let hook_dst = cfg.mnt.join("etc/initcpio/install/appsynergy-ssh-unlock");
    if hook_src.is_file() {
        fs::copy(hook_src, &hook_dst)?;
    }
    let _ = cmd::run(
        "server-overlay",
        "chmod",
        &["755", &hook_dst.to_string_lossy()],
    );

    // networkd initramfs unit for mkinitcpio-netconf systemd branch
    let net_init = Path::new("/etc/appsynergy/server/20-wired.network.initramfs");
    let net_dst = cfg.mnt.join("etc/systemd/network/20-wired.network.initramfs");
    if net_init.is_file() {
        fs::copy(net_init, &net_dst)?;
    } else {
        fs::write(
            &net_dst,
            "[Match]\nName=en* eth*\n\n[Network]\nDHCP=yes\n",
        )?;
    }

    // dropbear initrd unlock key (root + user authorized_keys already in install_ssh_keys)
    if let Some(ref key) = cfg.ssh_pubkey {
        fs::create_dir_all(cfg.mnt.join("etc/dropbear"))?;
        fs::write(cfg.mnt.join("etc/dropbear/root_key"), key)?;
        let _ = cmd::run(
            "server-overlay",
            "chmod",
            &["600", &cfg.mnt.join("etc/dropbear/root_key").to_string_lossy()],
        );
        println!("    dropbear root_key armed for initrd SSH unlock");
    } else {
        eprintln!("WARN: no baked SSH pubkey — initrd SSH unlock disarmed; root password login may still work until you harden");
        fs::write(
            cfg.mnt.join("etc/ssh/sshd_config.d/10-appsynergy.conf"),
            "PermitRootLogin yes\nPasswordAuthentication yes\nPubkeyAuthentication yes\nX11Forwarding no\n# Re-apply key-only after: install pubkey + set PasswordAuthentication no\n",
        )?;
    }

    // Order: modules-load before systemd-sysctl so conntrack keys exist
    let drop = cfg
        .mnt
        .join("etc/systemd/system/systemd-sysctl.service.d");
    fs::create_dir_all(&drop)?;
    fs::write(
        drop.join("10-after-modules-load.conf"),
        "[Unit]\nAfter=systemd-modules-load.service\nRequires=systemd-modules-load.service\n",
    )?;

    // Mask stock dropbear multi-user unit if present — only initrd uses dropbear.
    cmd::arch_chroot_ok(&cfg.mnt, "systemctl mask dropbear.service 2>/dev/null || true");

    fs::write(cfg.mnt.join("etc/appsynergy/VARIANT"), "server\n")?;
    fs::write(
        cfg.mnt.join("etc/appsynergy/UNLOCK.txt"),
        unlock_doc(cfg),
    )?;
    println!("    server overlay: sysctl, nft, sshd, watchdog, journald, SSH-unlock hooks");
    Ok(())
}

fn copy_or(src: &Path, dst: &Path, fallback: &str) -> Result<()> {
    if src.is_file() {
        fs::copy(src, dst)?;
    } else {
        fs::write(dst, fallback)?;
    }
    Ok(())
}

fn unlock_doc(cfg: &Config) -> String {
    format!(
        "AppSynergy Server disk unlock order\n\
====================================\n\
Layout: full-disk LUKS2 + btrfs (same as desktop).\n\
\n\
1) TPM2 (automatic)\n\
   systemd-cryptenroll token; crypttab tpm2-device=auto.\n\
   No network or SSH required when PCR policy matches.\n\
\n\
2) SSH initrd unlock (only if TPM fails / absent)\n\
   Initramfs starts network (ip=dhcp) + dropbear (key-only).\n\
   ssh root@<ip>  →  appsynergy-initrd-unlock  →  type LUKS passphrase\n\
   Requires install with --ssh-pubkey / APPSYNERGY_SSH_PUBKEY.\n\
   Armed: {}\n\
   Verify dropbear host key fingerprint on serial/IPMI before typing the passphrase.\n\
\n\
3) Local console / IPMI\n\
   Type the LUKS passphrase at the prompt (always available).\n\
\n\
Passphrase keyslot is NEVER removed when TPM enrolls.\n\
Re-enroll TPM: sudo appsynergy-tpm-enroll\n\
Rebuild initrd after key changes: sudo mkinitcpio -P\n\
\n\
NOT from appsynergy-linux (by design): agent, pets, console SPA, RAUC, verity, UKI.\n",
        if cfg.ssh_pubkey.is_some() {
            "yes"
        } else {
            "no — re-run install or add /etc/dropbear/root_key + mkinitcpio -P"
        }
    )
}

fn finalize(cfg: &Config) -> Result<()> {
    fs::create_dir_all(cfg.mnt.join("etc/appsynergy"))?;
    fs::write(
        cfg.mnt.join("etc/appsynergy/SHELL-POLICY.txt"),
        format!(
            "{}: system automation uses bash.\n\
fish is installed for interactive use only.\n\
Login shell default: /bin/bash\nVariant: {}\n",
            cfg.variant.product_name(),
            cfg.variant
        ),
    )?;
    let mut copy_files = vec![
        "BAZEL-HOST.txt",
        "README-INSTALL.txt",
        "packages-target.txt",
        "machine.env",
    ];
    if cfg.variant.is_server() {
        copy_files.extend_from_slice(&[
            "packages-target-server.txt",
            "machine-server.env",
            "sysctl-server.conf",
        ]);
    }
    for f in copy_files {
        let src = Path::new("/etc/appsynergy").join(f);
        if src.is_file() {
            let _ = fs::copy(&src, cfg.mnt.join("etc/appsynergy").join(f));
        }
    }
    if !cfg.variant.is_server() {
        cmd::arch_chroot_ok(
            &cfg.mnt,
            r#"
if command -v bazelisk >/dev/null 2>&1; then
  ln -sfn /usr/bin/bazelisk /usr/local/bin/bazelisk
  [[ -e /usr/local/bin/bazel ]] || ln -sfn /usr/bin/bazelisk /usr/local/bin/bazel
fi
"#,
        );
    }
    let tpm_enrolled = cfg.mnt.join("etc/appsynergy/tpm-enrolled").is_file();
    let product = cfg.variant.product_name();
    let motd = if cfg.variant.is_server() {
        if tpm_enrolled {
            format!(
                "\n  {product}\n  LUKS+btrfs · TPM unlock (passphrase recovery)\n  \
networkd + nftables + wireguard-tools · SSH :22\n  \
See /etc/appsynergy/TPM.txt and /etc/nftables.conf\n\n"
            )
        } else {
            format!(
                "\n  {product}\n  LUKS+btrfs · passphrase unlock (no TPM enrolled)\n  \
networkd + nftables + wireguard-tools · SSH :22\n  \
TPM later: sudo appsynergy-tpm-enroll — /etc/appsynergy/TPM.txt\n\n"
            )
        }
    } else if tpm_enrolled {
        format!(
            "\n  {product}\n  pacman -Syu to update. Kernel: see uname -r.\n  \
Disk: TPM unlock (passphrase still works). See /etc/appsynergy/TPM.txt\n\n"
        )
    } else {
        format!(
            "\n  {product}\n  pacman -Syu to update. Kernel: see uname -r.\n  \
TPM unlock: sudo appsynergy-tpm-enroll — see /etc/appsynergy/TPM.txt\n\n"
        )
    };
    fs::write(cfg.mnt.join("etc/motd"), motd)?;
    let luks_uuid = fs::read_to_string(cfg.mnt.join("etc/appsynergy/luks-uuid"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let tpm_doc = if tpm_enrolled {
        format!(
            "LUKS device: {}\n\
LUKS UUID:   {luks_uuid}\n\
Mapper:      {}\n\n\
TPM2 was enrolled at install time (PCRs: {}).\n\
Passphrase keyslot is ALWAYS kept as recovery.\n\n\
Expect automatic unlock when PCRs match. If stuck at passphrase, type the\n\
volume password (still valid).\n\n\
Re-enroll after firmware/Secure Boot policy changes:\n\
  sudo appsynergy-tpm-enroll\n\n\
List: systemd-cryptenroll {}\n\
Wipe TPM only: systemd-cryptenroll --wipe-slot=tpm2 {}\n",
            cfg.luks_part.display(),
            cfg.cryptname,
            cfg.tpm_pcrs,
            cfg.luks_part.display(),
            cfg.luks_part.display()
        )
    } else {
        format!(
            "LUKS device: {}\n\
LUKS UUID:   {luks_uuid}\n\
Mapper:      {}\n\n\
Passphrase: set during install. ALWAYS kept as recovery.\n\
TPM was not enrolled at install (no TPM, --no-tpm, or enroll failed).\n\n\
Enroll after a good passphrase boot:\n\n\
  sudo appsynergy-tpm-enroll\n\n\
Or manually:\n\n\
  sudo systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs={} {}\n\
  sudo mkinitcpio -P\n\
  reboot\n\n\
Never remove the passphrase slot.\n",
            cfg.luks_part.display(),
            cfg.cryptname,
            cfg.tpm_pcrs,
            cfg.luks_part.display()
        )
    };
    fs::write(cfg.mnt.join("etc/appsynergy/TPM.txt"), tpm_doc)?;
    for bin in ["appsynergy-tpm-enroll", "appsynergy-sanitize-mirrors"] {
        let src = Path::new("/usr/local/bin").join(bin);
        if src.is_file() {
            let dest = cfg.mnt.join("usr/local/bin").join(bin);
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::copy(&src, &dest)?;
            let _ = cmd::run(
                "finalize",
                "chmod",
                &["755", &dest.to_string_lossy()],
            );
        }
    }
    // Plasma skel (desktop only)
    if !cfg.variant.is_server() {
        fs::create_dir_all(cfg.mnt.join("etc/skel/.config"))?;
        fs::create_dir_all(cfg.mnt.join("etc/sddm.conf.d"))?;
        if Path::new("/etc/skel/.config/kdeglobals").is_file() {
            let _ = fs::copy(
                "/etc/skel/.config/kdeglobals",
                cfg.mnt.join("etc/skel/.config/kdeglobals"),
            );
        }
        if Path::new("/etc/sddm.conf.d/appsynergy.conf").is_file() {
            let _ = fs::copy(
                "/etc/sddm.conf.d/appsynergy.conf",
                cfg.mnt.join("etc/sddm.conf.d/appsynergy.conf"),
            );
        }
        let home = cfg.mnt.join("home").join(&cfg.user);
        if home.is_dir() {
            fs::create_dir_all(home.join(".config"))?;
            if Path::new("/etc/skel/.config/kdeglobals").is_file() {
                let _ = fs::copy(
                    "/etc/skel/.config/kdeglobals",
                    home.join(".config/kdeglobals"),
                );
            }
            cmd::arch_chroot_ok(
                &cfg.mnt,
                &format!("chown -R {u}:{u} /home/{u}/.config", u = cfg.user),
            );
        }
    }
    let _ = cmd::run("finalize", "sync", &[]);
    Ok(())
}
