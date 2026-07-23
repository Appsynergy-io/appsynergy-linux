//! AppSynergy Linux full-disk installer (live USB).
//!
//! Orchestrates partitioning, LUKS, pacstrap, branding, and boot setup.
//! Fixes from INSTALL-PROBLEMS.md (2026-07-23):
//! - os-release: write `/usr/lib/os-release` only; symlink `/etc/os-release`
//! - branding: no pre-seed of package-owned files; `pacman -U --overwrite '*'`
//! - `--password-file` / `APPSYNERGY_KEYFILE` for LUKS + chpasswd
//! - `/etc/vconsole.conf` before first mkinitcpio
//! - `efibootmgr` NVRAM entry for new ESP; drop stale PARTUUIDs
//! - every failure includes the step name

mod cmd;
mod config;

use anyhow::{bail, Context, Result};
use clap::Parser;
use config::{Cli, Config, KernelMode, APPSYNERGY_REPO};
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
    let cli = Cli::parse();
    let cfg = Config::load(cli)?;

    if !is_root() {
        bail!("run as root");
    }
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
    if !cfg.disk.exists() {
        bail!("not a block device: {}", cfg.disk.display());
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
    step("pacstrap", || pacstrap_packages(&cfg))?;
    step("local-kernel", || install_local_kernel(&cfg))?;
    step("appsynergy-repo", || register_appsynergy_repo(&cfg))?;
    step("branding", || install_branding(&cfg))?;
    step("browsers", || install_browsers(&cfg))?;
    step("fstab-crypttab", || fstab_crypttab(&cfg))?;
    step("locale", || locale_hostname(&cfg))?;
    step("os-release", || apply_os_release(&cfg))?;
    step("network-bt", || network_and_bluetooth(&cfg))?;
    step("mkinitcpio-config", || configure_mkinitcpio(&cfg))?;
    step("users", || create_users(&cfg))?;
    step("bootloader", || install_bootloader(&cfg))?;
    step("efibootmgr", || fix_efi_nvram(&cfg))?;
    step("initramfs", || rebuild_initramfs(&cfg))?;
    step("services", || enable_services(&cfg))?;
    step("finalize", || finalize(&cfg))?;

    println!();
    println!("============================================================");
    println!("  INSTALL COMPLETE");
    println!("  1. reboot  (remove USB)");
    println!("  2. At unlock prompt: type VOLUME passphrase");
    println!("  3. Login as {}", cfg.user);
    println!("  4. Later: sudo appsynergy-tpm-enroll");
    println!("============================================================");
    println!(
        "Unmount: umount -R {} && cryptsetup close {}",
        cfg.mnt.display(),
        cfg.cryptname
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
    println!("  AppSynergy Linux installer — FULL DISK WIPE (rust)");
    println!("============================================================");
    println!("  disk:       {}", cfg.disk.display());
    println!("  EFI:        {} ({})", cfg.efi_part.display(), cfg.efi_size);
    println!("  LUKS+btrfs: {}", cfg.luks_part.display());
    println!("  hostname:   {}", cfg.hostname);
    println!("  user:       {} (bash login; fish installed)", cfg.user);
    println!(
        "  locale:     {}  keymap: {}  tz: {}",
        cfg.locale, cfg.keymap, cfg.timezone
    );
    println!("  kernel:     {}", cfg.kernel);
    println!(
        "  password:   {}",
        if cfg.password.is_some() {
            "from keyfile (non-interactive)"
        } else {
            "interactive prompts"
        }
    );
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
    print!(
        "Type the disk path exactly to continue ({}): ",
        cfg.disk.display()
    );
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    if line.trim() != cfg.disk.to_string_lossy() {
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
    let disk = cfg.disk.to_string_lossy();
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
    thread::sleep(Duration::from_secs(1));
    if !cfg.efi_part.exists() || !cfg.luks_part.exists() {
        bail!(
            "partitions not found after sgdisk ({} / {})",
            cfg.efi_part.display(),
            cfg.luks_part.display()
        );
    }
    Ok(())
}

fn luks_format_open(cfg: &Config) -> Result<()> {
    let part = cfg.luks_part.to_string_lossy();
    if let Some(ref pw) = cfg.password {
        // Non-interactive: --key-file=- and passphrase on stdin (batch mode)
        println!("    LUKS format via keyfile (non-interactive)");
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
            &["open", "--key-file=-", &part, &cfg.cryptname],
            pw,
        )?;
    } else {
        println!("    You will type a NEW volume passphrase TWICE.");
        println!("    TPM unlock is enrolled AFTER first successful boot.");
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
        println!("==> Opening LUKS (type the same passphrase once more)");
        cmd::run("luks", "cryptsetup", &["open", &part, &cfg.cryptname])?;
    }
    Ok(())
}

fn filesystems(cfg: &Config) -> Result<()> {
    cmd::run(
        "filesystems",
        "mkfs.fat",
        &["-F32", "-n", "EFI", &cfg.efi_part.to_string_lossy()],
    )?;
    let mapper = format!("/dev/mapper/{}", cfg.cryptname);
    cmd::run(
        "filesystems",
        "mkfs.btrfs",
        &["-f", "-L", "appsynergy", &mapper],
    )?;
    Ok(())
}

fn btrfs_subvols(cfg: &Config) -> Result<()> {
    let mapper = format!("/dev/mapper/{}", cfg.cryptname);
    cmd::run("subvolumes", "mount", &[&mapper, "/mnt"])?;
    for sv in ["@", "@home", "@log", "@cache", "@snapshots"] {
        cmd::run("subvolumes", "btrfs", &["subvolume", "create", &format!("/mnt/{sv}")])?;
    }
    cmd::run("subvolumes", "umount", &["/mnt"])?;

    fs::create_dir_all(&cfg.mnt)?;
    let mnt = cfg.mnt.to_string_lossy();
    cmd::run(
        "subvolumes",
        "mount",
        &[
            "-o",
            "subvol=@,compress=zstd:3,noatime",
            &mapper,
            &mnt,
        ],
    )?;
    for d in ["boot", "home", "var/log", "var/cache", ".snapshots"] {
        fs::create_dir_all(cfg.mnt.join(d))?;
    }
    cmd::run(
        "subvolumes",
        "mount",
        &[
            "-o",
            "subvol=@home,compress=zstd:3,noatime",
            &mapper,
            &format!("{mnt}/home"),
        ],
    )?;
    cmd::run(
        "subvolumes",
        "mount",
        &[
            "-o",
            "subvol=@log,compress=zstd:3,noatime",
            &mapper,
            &format!("{mnt}/var/log"),
        ],
    )?;
    cmd::run(
        "subvolumes",
        "mount",
        &[
            "-o",
            "subvol=@cache,compress=zstd:3,noatime",
            &mapper,
            &format!("{mnt}/var/cache"),
        ],
    )?;
    cmd::run(
        "subvolumes",
        "mount",
        &[
            "-o",
            "subvol=@snapshots,compress=zstd:3,noatime",
            &mapper,
            &format!("{mnt}/.snapshots"),
        ],
    )?;
    cmd::run(
        "subvolumes",
        "mount",
        &[&cfg.efi_part.to_string_lossy(), &format!("{mnt}/boot")],
    )?;
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
    let (pkg, hdr) = find_kernel_pkgs(dir)?;
    let dest = cfg.mnt.join("root/pkgs");
    fs::create_dir_all(&dest)?;
    fs::copy(&pkg, dest.join(pkg.file_name().unwrap()))?;
    fs::copy(&hdr, dest.join(hdr.file_name().unwrap()))?;
    cmd::arch_chroot(
        &cfg.mnt,
        "pacman -U --noconfirm /root/pkgs/*.pkg.tar.zst",
    )?;
    Ok(())
}

fn find_kernel_pkgs(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let try_pair = |prefix: &str| -> Option<(PathBuf, PathBuf)> {
        let mut pkgs = list_glob(dir, &format!("{prefix}-[0-9]*.pkg.tar.zst"));
        let mut hdrs = list_glob(dir, &format!("{prefix}-headers-*.pkg.tar.zst"));
        pkgs.retain(|p| !p.to_string_lossy().contains("-dbg-"));
        hdrs.retain(|p| !p.to_string_lossy().contains("-dbg-"));
        if !pkgs.is_empty() && !hdrs.is_empty() {
            Some((pkgs[0].clone(), hdrs[0].clone()))
        } else {
            None
        }
    };
    try_pair("linux-appsynergy")
        .or_else(|| try_pair("linux-cachyos-igpu"))
        .with_context(|| {
            format!(
                "kernel mode local but missing pkgs in {} (need linux-appsynergy or linux-cachyos-igpu + headers)",
                dir.display()
            )
        })
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

    let luks_uuid = cmd::output(
        "fstab",
        "blkid",
        &["-s", "UUID", "-o", "value", &cfg.luks_part.to_string_lossy()],
    )?;
    let crypttab = format!(
        "{} UUID={} none luks,discard,x-initrd.attach\n",
        cfg.cryptname, luks_uuid
    );
    fs::write(cfg.mnt.join("etc/crypttab"), &crypttab)?;
    let _ = fs::write(cfg.mnt.join("etc/crypttab.initramfs"), &crypttab);
    // stash for later steps
    fs::create_dir_all(cfg.mnt.join("etc/appsynergy"))?;
    fs::write(cfg.mnt.join("etc/appsynergy/luks-uuid"), format!("{luks_uuid}\n"))?;
    fs::write(
        cfg.mnt.join("etc/appsynergy/luks-device"),
        format!("{}\n", cfg.luks_part.display()),
    )?;
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
    let lib = cfg.mnt.join("usr/lib/os-release");
    let etc = cfg.mnt.join("etc/os-release");

    if let Some(src) = src {
        fs::copy(&src, &lib).with_context(|| format!("copy {} -> {}", src.display(), lib.display()))?;
    } else {
        fs::write(
            &lib,
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
"#,
        )?;
    }
    // Replace /etc/os-release with a relative symlink (Arch convention).
    if etc.exists() || etc.symlink_metadata().is_ok() {
        fs::remove_file(&etc).ok();
    }
    std::os::unix::fs::symlink("../usr/lib/os-release", &etc)
        .with_context(|| format!("symlink {}", etc.display()))?;
    println!("    wrote /usr/lib/os-release; /etc/os-release -> ../usr/lib/os-release");
    Ok(())
}

fn network_and_bluetooth(cfg: &Config) -> Result<()> {
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
    let hooks = "HOOKS=(base systemd autodetect microcode modconf kms keyboard sd-vconsole block sd-encrypt filesystems fsck)";
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
    // hid_apple module
    if !text.contains("hid_apple") {
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

fn create_users(cfg: &Config) -> Result<()> {
    let groups = "wheel,docker,audio,input,video,lp,rfkill,storage,network,tss,uucp";
    if let Some(ref pw) = cfg.password {
        // chpasswd expects user:password lines
        let root_line = format!("root:{}\n", String::from_utf8_lossy(pw));
        let user_line = format!("{}:{}\n", cfg.user, String::from_utf8_lossy(pw));
        // create user first without password
        let script = format!(
            r#"
set -euo pipefail
id '{user}' >/dev/null 2>&1 || useradd -m -G {groups} -s /bin/bash '{user}'
usermod -aG {groups} '{user}' || true
mkdir -p /etc/sudoers.d
echo '%wheel ALL=(ALL:ALL) ALL' > /etc/sudoers.d/wheel
chmod 440 /etc/sudoers.d/wheel
for pamf in /etc/pam.d/sddm /etc/pam.d/sddm-autologin; do
  [[ -f $pamf ]] || continue
  if ! grep -q pam_kwallet5.so $pamf; then
    echo '-auth       optional    pam_kwallet5.so' >> $pamf
    echo '-session    optional    pam_kwallet5.so auto_start' >> $pamf
  fi
done
"#,
            user = cfg.user,
            groups = groups,
        );
        cmd::arch_chroot(&cfg.mnt, &script)?;
        // chpasswd via arch-chroot with stdin is awkward; write a temp file in chroot
        let pwfile = cfg.mnt.join("root/.appsynergy-chpasswd");
        let mut body = root_line;
        body.push_str(&user_line);
        fs::write(&pwfile, body.as_bytes())?;
        // restrict
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
for pamf in /etc/pam.d/sddm /etc/pam.d/sddm-autologin; do
  [[ -f $pamf ]] || continue
  if ! grep -q pam_kwallet5.so $pamf; then
    echo '-auth       optional    pam_kwallet5.so' >> $pamf
    echo '-session    optional    pam_kwallet5.so auto_start' >> $pamf
  fi
done
"#,
            user = cfg.user,
            groups = groups,
        );
        cmd::arch_chroot(&cfg.mnt, &script)?;
    }
    Ok(())
}

fn install_bootloader(cfg: &Config) -> Result<()> {
    cmd::arch_chroot(&cfg.mnt, "bootctl install")?;
    let vmlinuz = cmd::output(
        "bootloader",
        "arch-chroot",
        &[
            &cfg.mnt.to_string_lossy(),
            "bash",
            "-c",
            "ls /boot/vmlinuz-* | head -1 | xargs -n1 basename",
        ],
    )?;
    let initramfs = cmd::output(
        "bootloader",
        "arch-chroot",
        &[
            &cfg.mnt.to_string_lossy(),
            "bash",
            "-c",
            "ls /boot/initramfs-*.img | grep -v fallback | head -1 | xargs -n1 basename",
        ],
    )?;
    if vmlinuz.is_empty() || initramfs.is_empty() {
        bail!("could not find kernel images in /boot");
    }
    let luks_uuid = fs::read_to_string(cfg.mnt.join("etc/appsynergy/luks-uuid"))?
        .trim()
        .to_string();
    fs::create_dir_all(cfg.mnt.join("boot/loader/entries"))?;
    fs::write(
        cfg.mnt.join("boot/loader/loader.conf"),
        "default appsynergy.conf\ntimeout 3\nconsole-mode keep\n",
    )?;
    fs::write(
        cfg.mnt.join("boot/loader/entries/appsynergy.conf"),
        format!(
            "title   AppSynergy Linux\n\
linux   /{vmlinuz}\n\
initrd  /intel-ucode.img\n\
initrd  /{initramfs}\n\
options rd.luks.name={luks_uuid}={} root=/dev/mapper/{} rootflags=subvol=@ rw zswap.enabled=0 nowatchdog\n",
            cfg.cryptname, cfg.cryptname
        ),
    )?;
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

    // Create / refresh AppSynergy entry pointing at this disk+partition.
    let disk = cfg.disk.to_string_lossy();
    // partition number: last digits of efi part name
    let part_num = cfg
        .efi_part
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| {
            s.chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(1);

    // Remove existing "AppSynergy Linux" entries first to avoid duplicates
    if let Ok(list) = cmd::output("efibootmgr", "efibootmgr", &["-v"]) {
        for line in list.lines() {
            if line.contains("AppSynergy Linux") {
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
            "AppSynergy Linux",
            "-l",
            r"\EFI\systemd\systemd-bootx64.efi",
        ],
    )?;
    println!("    NVRAM entry AppSynergy Linux -> ESP {partuuid}");
    Ok(())
}

fn rebuild_initramfs(cfg: &Config) -> Result<()> {
    // vconsole already present
    cmd::arch_chroot(&cfg.mnt, "mkinitcpio -P")?;
    Ok(())
}

fn enable_services(cfg: &Config) -> Result<()> {
    cmd::arch_chroot_ok(
        &cfg.mnt,
        "systemctl enable NetworkManager sddm sshd docker fstrim.timer bluetooth || true",
    );
    cmd::arch_chroot_ok(&cfg.mnt, "systemctl enable obex || true");
    cmd::arch_chroot_ok(
        &cfg.mnt,
        "systemctl --global enable plasma-kwallet-pam.service || true",
    );
    Ok(())
}

fn finalize(cfg: &Config) -> Result<()> {
    fs::create_dir_all(cfg.mnt.join("etc/appsynergy"))?;
    fs::write(
        cfg.mnt.join("etc/appsynergy/SHELL-POLICY.txt"),
        "AppSynergy Linux: system and agent automation use bash.\n\
fish is installed for interactive use only.\n\
Login shell default: /bin/bash\n",
    )?;
    for f in [
        "BAZEL-HOST.txt",
        "README-INSTALL.txt",
        "packages-target.txt",
        "machine.env",
    ] {
        let src = Path::new("/etc/appsynergy").join(f);
        if src.is_file() {
            let _ = fs::copy(&src, cfg.mnt.join("etc/appsynergy").join(f));
        }
    }
    cmd::arch_chroot_ok(
        &cfg.mnt,
        r#"
if command -v bazelisk >/dev/null 2>&1; then
  ln -sfn /usr/bin/bazelisk /usr/local/bin/bazelisk
  [[ -e /usr/local/bin/bazel ]] || ln -sfn /usr/bin/bazelisk /usr/local/bin/bazel
fi
"#,
    );
    fs::write(
        cfg.mnt.join("etc/motd"),
        "\n  AppSynergy Linux\n  pacman -Syu to update. Kernel: see uname -r.\n  TPM unlock: after a good passphrase boot, see /etc/appsynergy/TPM.txt\n\n",
    )?;
    let luks_uuid = fs::read_to_string(cfg.mnt.join("etc/appsynergy/luks-uuid"))
        .unwrap_or_default()
        .trim()
        .to_string();
    fs::write(
        cfg.mnt.join("etc/appsynergy/TPM.txt"),
        format!(
            "LUKS device: {}\n\
LUKS UUID:   {luks_uuid}\n\
Mapper:      {}\n\n\
Passphrase: set during install. ALWAYS kept as recovery.\n\n\
TPM unlock (after 1–2 good passphrase boots):\n\n\
  sudo appsynergy-tpm-enroll\n\n\
Or manually:\n\n\
  sudo systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7 {}\n\
  sudo mkinitcpio -P\n\
  reboot\n\n\
Never remove the passphrase slot.\n",
            cfg.luks_part.display(),
            cfg.cryptname,
            cfg.luks_part.display()
        ),
    )?;
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
    // Plasma skel
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
            &format!(
                "chown -R {u}:{u} /home/{u}/.config",
                u = cfg.user
            ),
        );
    }
    let _ = cmd::run("finalize", "sync", &[]);
    Ok(())
}
