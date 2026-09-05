//! Interactive install guide — few questions, sensible defaults.
//! Includes explicit **nuke existing install** confirmation when disks look occupied.
//! Batch: `--yes` + flags. Skip guide: `APPSYNERGY_NO_GUIDE=1`.

use crate::config::{Cli, Variant};
use anyhow::{bail, Result};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// True when we should run the wizard (not `--yes`, not NO_GUIDE, TTY available).
pub fn should_guide(cli: &Cli) -> bool {
    should_guide_with(
        cli.yes,
        std::env::var_os("APPSYNERGY_NO_GUIDE").is_some(),
        atty_stdin(),
    )
}

/// Pure: guide when not batch, not forced off, and interactive.
pub fn should_guide_with(yes: bool, no_guide_env: bool, has_tty: bool) -> bool {
    !yes && !no_guide_env && has_tty
}

fn atty_stdin() -> bool {
    std::fs::File::open("/dev/tty").is_ok()
}

/// Parse menu choice for product variant.
pub fn parse_variant_choice(input: &str) -> Result<Variant> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "d" | "desktop" => Ok(Variant::Desktop),
        "2" | "s" | "server" | "" => Ok(Variant::Server),
        other => bail!("expected 1 (Desktop) or 2 (Server), got {other:?}"),
    }
}

/// Y/n (empty = default_yes).
pub fn parse_yes_no(input: &str, default_yes: bool) -> bool {
    match input.trim().to_ascii_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        "n" | "no" => false,
        // treat unknown as default
        _ => default_yes,
    }
}

/// Disk decision after dual-NVMe prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskChoice {
    Raid1 { disks: String },
    Single { disk: PathBuf },
}

/// Detect signs of an existing OS/install on planned disks (for nuke prompt).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExistingInstallHint {
    pub has_partitions: bool,
    pub has_luks: bool,
    pub has_appsynergy: bool,
    pub details: Vec<String>,
}

impl ExistingInstallHint {
    pub fn looks_occupied(&self) -> bool {
        self.has_partitions || self.has_luks || self.has_appsynergy
    }
}

/// Pure parse of `lsblk -o NAME,FSTYPE,LABEL,PARTLABEL` style lines for one disk basename.
pub fn parse_lsblk_for_existing(lsblk_text: &str, disk_basenames: &[&str]) -> ExistingInstallHint {
    let mut h = ExistingInstallHint::default();
    for line in lsblk_text.lines() {
        let lower = line.to_ascii_lowercase();
        let on_disk = disk_basenames
            .iter()
            .any(|b| lower.contains(&b.to_ascii_lowercase()));
        if !on_disk {
            continue;
        }
        // child partitions (contain 'p' digit or disk name with partition)
        if line.contains("part") || line.contains("├") || line.contains("└") || line.contains("─")
        {
            h.has_partitions = true;
        }
        if lower.contains("crypto_luks") || lower.contains("luks") {
            h.has_luks = true;
            h.details.push(line.trim().to_string());
        }
        if lower.contains("appsynergy") {
            h.has_appsynergy = true;
            h.details.push(line.trim().to_string());
        }
        // any non-empty fstype on a partition line
        if lower.contains("vfat")
            || lower.contains("btrfs")
            || lower.contains("ext4")
            || lower.contains("xfs")
        {
            h.has_partitions = true;
            h.details.push(line.trim().to_string());
        }
    }
    h
}

/// Probe disks with lsblk (best-effort).
pub fn probe_existing_install(disks: &[PathBuf]) -> ExistingInstallHint {
    let mut combined = ExistingInstallHint::default();
    let basenames: Vec<String> = disks
        .iter()
        .filter_map(|d| {
            d.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect();
    let basenames_ref: Vec<&str> = basenames.iter().map(|s| s.as_str()).collect();

    for disk in disks {
        let out = Command::new("lsblk")
            .args([
                "-o",
                "NAME,FSTYPE,LABEL,PARTLABEL,TYPE",
                "-n",
                &disk.to_string_lossy(),
            ])
            .output();
        if let Ok(o) = out {
            let text = String::from_utf8_lossy(&o.stdout);
            let h = parse_lsblk_for_existing(&text, &basenames_ref);
            combined.has_partitions |= h.has_partitions;
            combined.has_luks |= h.has_luks;
            combined.has_appsynergy |= h.has_appsynergy;
            combined.details.extend(h.details);
        }
    }
    combined
}

/// Nuke confirmation: must type exactly NUKE (or default reject).
pub fn parse_nuke_confirm(input: &str) -> bool {
    input.trim() == "NUKE"
}

/// Password pair validation for wizard.
pub fn validate_password_pair(p1: &str, p2: &str) -> Result<()> {
    if p1 != p2 {
        bail!("passwords do not match");
    }
    Ok(())
}

/// Apply disk choice onto CLI.
pub fn apply_disk_choice(cli: &mut Cli, choice: DiskChoice) {
    match choice {
        DiskChoice::Raid1 { disks } => {
            cli.disks = Some(disks);
            cli.disk = None;
        }
        DiskChoice::Single { disk } => {
            cli.disk = Some(disk);
            cli.disks = None;
        }
    }
}

/// Resolve planned disk paths from CLI (for probe).
pub fn planned_disks_from_cli(cli: &Cli) -> Vec<PathBuf> {
    if let Some(ref d) = cli.disks {
        return crate::disk::parse_disks_list(d).unwrap_or_default();
    }
    if let Some(ref d) = cli.disk {
        return vec![d.clone()];
    }
    let mut v = Vec::new();
    if Path::new("/dev/nvme0n1").exists() {
        v.push(PathBuf::from("/dev/nvme0n1"));
    }
    if Path::new("/dev/nvme1n1").exists() {
        v.push(PathBuf::from("/dev/nvme1n1"));
    }
    if v.is_empty() {
        if let Some(p) = first_existing(&["/dev/sda", "/dev/vda"]) {
            v.push(PathBuf::from(p));
        }
    }
    v
}

/// Fill in missing CLI fields via short prompts. Mutates `cli`.
pub fn run(cli: &mut Cli) -> Result<()> {
    println!();
    println!("============================================================");
    println!("  AppSynergy install");
    println!("============================================================");
    println!();

    // --- 1. What to install ---
    if cli.variant.is_none() {
        println!("What are you installing?");
        println!("  1) Desktop  — workstation (Plasma)");
        println!("  2) Server   — headless tunnels / OVH");
        println!();
        let choice = prompt("Enter 1 or 2", "2")?;
        cli.variant = Some(parse_variant_choice(&choice)?);
    }

    let server = matches!(cli.variant, Some(Variant::Server));

    // --- 2. Disks ---
    if server && cli.disks.is_none() && cli.disk.is_none() {
        let dual = Path::new("/dev/nvme0n1").exists() && Path::new("/dev/nvme1n1").exists();
        if dual {
            println!();
            println!("Found two NVMe disks:");
            println!("  /dev/nvme0n1");
            println!("  /dev/nvme1n1");
            println!("Server layout: LUKS on each + btrfs RAID1 (mirrored).");
            let a = prompt("Use both disks (RAID1)? [Y/n]", "Y")?;
            if parse_yes_no(&a, true) {
                apply_disk_choice(
                    cli,
                    DiskChoice::Raid1 {
                        disks: "/dev/nvme0n1,/dev/nvme1n1".into(),
                    },
                );
            } else {
                let d = prompt("Single disk path", "/dev/nvme0n1")?;
                apply_disk_choice(
                    cli,
                    DiskChoice::Single {
                        disk: PathBuf::from(d.trim()),
                    },
                );
            }
        } else {
            let default = first_existing(&["/dev/nvme0n1", "/dev/sda", "/dev/vda"])
                .unwrap_or_else(|| "/dev/sda".into());
            println!();
            println!("Disk to wipe (full install):");
            let d = prompt(&format!("Disk path [{default}]"), &default)?;
            apply_disk_choice(
                cli,
                DiskChoice::Single {
                    disk: PathBuf::from(d.trim()),
                },
            );
        }
    } else if !server && cli.disk.is_none() {
        let default =
            first_existing(&["/dev/nvme0n1", "/dev/sda"]).unwrap_or_else(|| "/dev/nvme0n1".into());
        println!();
        let d = prompt(&format!("Disk path [{default}]"), &default)?;
        apply_disk_choice(
            cli,
            DiskChoice::Single {
                disk: PathBuf::from(d.trim()),
            },
        );
    }

    // --- 2b. Nuke existing install ---
    let planned = planned_disks_from_cli(cli);
    if !planned.is_empty() {
        let hint = probe_existing_install(&planned);
        if hint.looks_occupied() {
            println!();
            println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
            println!("  Existing data detected on target disk(s):");
            if hint.has_appsynergy {
                println!("    • AppSynergy (or labeled appsynergy) already present");
            }
            if hint.has_luks {
                println!("    • LUKS encrypted volume(s)");
            }
            if hint.has_partitions {
                println!("    • partitions / filesystems");
            }
            for d in hint.details.iter().take(8) {
                println!("      {d}");
            }
            println!();
            println!("  To wipe everything and reinstall, type exactly:  NUKE");
            println!("  Anything else aborts (keeps current disks).");
            println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
            let ans = prompt("Type NUKE to destroy existing install", "")?;
            if !parse_nuke_confirm(&ans) {
                bail!("aborted — existing install not nuked (type NUKE to confirm wipe)");
            }
            println!("  OK — will NUKE existing install and write a new one.");
        } else {
            println!();
            println!("Target disk(s) look empty/uninitialized (or probe skipped).");
            let a = prompt("Continue and wipe them? [Y/n]", "Y")?;
            if !parse_yes_no(&a, true) {
                bail!("aborted by user");
            }
        }
    }

    // --- 3. Password ---
    if cli.password_file.is_none() && !Path::new("/tmp/appsynergy-key").is_file() {
        println!();
        println!("LUKS + root + login password (same for all).");
        println!("Leave empty to type passwords later during install.");
        let p1 = prompt_secret("Password (hidden, empty=later)")?;
        if !p1.is_empty() {
            let p2 = prompt_secret("Again")?;
            validate_password_pair(&p1, &p2)?;
            let path = PathBuf::from("/tmp/appsynergy-key");
            std::fs::write(&path, p1.as_bytes())?;
            let _ = Command::new("chmod")
                .args(["600", &path.to_string_lossy()])
                .status();
            cli.password_file = Some(path);
            println!("  (saved to /tmp/appsynergy-key for this install only)");
        }
    }

    // --- 4. SSH key: baked operator pubkey preferred; no generation ---
    if cli.ssh_pubkey.is_none() {
        let baked = Path::new("/etc/appsynergy/ssh-unlock.pub");
        if baked.is_file() {
            println!();
            println!("SSH: using baked operator pubkey ({})", baked.display());
            cli.ssh_pubkey = Some(baked.to_path_buf());
        } else if server {
            let found = [
                "/root/.ssh/authorized_keys",
                "/root/id_rsa.pub",
                "/root/id_ed25519.pub",
            ]
            .into_iter()
            .find(|p| Path::new(p).is_file());
            println!();
            if let Some(p) = found {
                println!("SSH public key found: {p}");
                let a = prompt("Use it for root + disk unlock? [Y/n]", "Y")?;
                if parse_yes_no(&a, true) {
                    cli.ssh_pubkey = Some(PathBuf::from(p));
                }
            }
            if cli.ssh_pubkey.is_none() {
                println!("SSH public key (root login + unlock if TPM fails).");
                println!("Path to existing .pub file, or empty to skip. (never generates a key)");
                let p = prompt("Path to .pub", "")?;
                let p = p.trim();
                if !p.is_empty() {
                    if !Path::new(p).is_file() {
                        bail!("not a file: {p}");
                    }
                    cli.ssh_pubkey = Some(PathBuf::from(p));
                } else {
                    println!("  (no key — unlock with passphrase on console/IPMI)");
                }
            }
        }
    }

    // --- 5. Summary ---
    let ksel = crate::detect::select_kernel_live(server);
    println!();
    println!("------------------------------------------------------------");
    println!(
        "  Install:  {}",
        match cli.variant {
            Some(Variant::Server) => "SERVER",
            Some(Variant::Desktop) => "DESKTOP",
            None => "?",
        }
    );
    if let Some(ref d) = cli.disks {
        println!("  Disks:    {d}  (RAID1) — NUKE existing");
    } else if let Some(ref d) = cli.disk {
        println!("  Disk:     {} — NUKE existing", d.display());
    }
    println!(
        "  CPU:      {} ({})",
        if ksel.cpu_model.is_empty() {
            "unknown"
        } else {
            ksel.cpu_model.as_str()
        },
        ksel.family_label
    );
    println!(
        "  Kernel:   {}",
        if ksel.pkg_prefixes.is_empty() {
            "(no host-max package for this CPU — install will error)".into()
        } else {
            ksel.pkg_prefixes.join(", ")
        }
    );
    println!("  FDE:      LUKS2 full-disk");
    println!(
        "  TPM:      {}",
        if crate::detect::tpm_present() {
            "present → auto-enroll at install (passphrase kept)"
        } else {
            "not present → passphrase unlock only"
        }
    );
    println!(
        "  Password: {}",
        if cli.password_file.is_some() || Path::new("/tmp/appsynergy-key").is_file() {
            "set"
        } else {
            "ask during install"
        }
    );
    if server {
        println!(
            "  SSH key:  {}",
            cli.ssh_pubkey
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none".into())
        );
    }
    println!("------------------------------------------------------------");
    println!("  Next: type YES when asked to destroy all data.");
    println!();
    Ok(())
}

fn first_existing(cands: &[&str]) -> Option<String> {
    cands
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|s| (*s).to_string())
}

fn prompt(label: &str, default: &str) -> Result<String> {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let t = line.trim();
    if t.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(t.to_string())
    }
}

fn prompt_secret(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let _ = Command::new("stty").args(["-echo"]).status();
    let mut line = String::new();
    let r = io::stdin().read_line(&mut line);
    let _ = Command::new("stty").args(["echo"]).status();
    println!();
    r?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Cli;
    use clap::Parser;

    fn empty_cli() -> Cli {
        Cli::parse_from(["appsynergy-install"])
    }

    #[test]
    fn should_guide_false_when_yes() {
        assert!(!should_guide_with(true, false, true));
    }

    #[test]
    fn should_guide_false_when_no_guide_env() {
        assert!(!should_guide_with(false, true, true));
    }

    #[test]
    fn should_guide_false_without_tty() {
        assert!(!should_guide_with(false, false, false));
    }

    #[test]
    fn should_guide_true_interactive() {
        assert!(should_guide_with(false, false, true));
    }

    #[test]
    fn variant_choice_server_default_empty() {
        assert_eq!(parse_variant_choice("").unwrap(), Variant::Server);
        assert_eq!(parse_variant_choice("2").unwrap(), Variant::Server);
        assert_eq!(parse_variant_choice("server").unwrap(), Variant::Server);
        assert_eq!(parse_variant_choice("S").unwrap(), Variant::Server);
    }

    #[test]
    fn variant_choice_desktop() {
        assert_eq!(parse_variant_choice("1").unwrap(), Variant::Desktop);
        assert_eq!(parse_variant_choice("desktop").unwrap(), Variant::Desktop);
        assert_eq!(parse_variant_choice("D").unwrap(), Variant::Desktop);
    }

    #[test]
    fn variant_choice_invalid() {
        assert!(parse_variant_choice("3").is_err());
        assert!(parse_variant_choice("laptop").is_err());
    }

    #[test]
    fn yes_no_defaults() {
        assert!(parse_yes_no("", true));
        assert!(!parse_yes_no("", false));
        assert!(parse_yes_no("Y", false));
        assert!(!parse_yes_no("n", true));
        assert!(parse_yes_no("yes", true));
    }

    #[test]
    fn apply_disk_choice_sets_cli() {
        let mut cli = empty_cli();
        apply_disk_choice(
            &mut cli,
            DiskChoice::Raid1 {
                disks: "/dev/nvme0n1,/dev/nvme1n1".into(),
            },
        );
        assert_eq!(cli.disks.as_deref(), Some("/dev/nvme0n1,/dev/nvme1n1"));
        assert!(cli.disk.is_none());

        apply_disk_choice(
            &mut cli,
            DiskChoice::Single {
                disk: PathBuf::from("/dev/sda"),
            },
        );
        assert_eq!(cli.disk, Some(PathBuf::from("/dev/sda")));
        assert!(cli.disks.is_none());
    }

    #[test]
    fn nuke_confirm_exact() {
        assert!(parse_nuke_confirm("NUKE"));
        assert!(parse_nuke_confirm("  NUKE  ")); // whitespace trimmed
        assert!(!parse_nuke_confirm("nuke")); // case-sensitive
        assert!(!parse_nuke_confirm("YES"));
        assert!(!parse_nuke_confirm(""));
        assert!(!parse_nuke_confirm("NUKE IT"));
    }

    #[test]
    fn lsblk_detects_appsynergy_and_luks() {
        let sample = r#"
nvme0n1
├─nvme0n1p1 vfat     EFI
└─nvme0n1p2 crypto_LUKS
nvme1n1
└─nvme1n1p2 btrfs    appsynergy-server
"#;
        let h = parse_lsblk_for_existing(sample, &["nvme0n1", "nvme1n1"]);
        assert!(h.looks_occupied());
        assert!(h.has_luks);
        assert!(h.has_appsynergy);
        assert!(h.has_partitions);
    }

    #[test]
    fn lsblk_empty_disk_not_occupied() {
        let sample = "nvme0n1\n";
        let h = parse_lsblk_for_existing(sample, &["nvme0n1"]);
        assert!(!h.looks_occupied());
    }

    #[test]
    fn password_pair_mismatch() {
        assert!(validate_password_pair("a", "b").is_err());
        assert!(validate_password_pair("x", "x").is_ok());
    }

    #[test]
    fn planned_disks_from_disks_flag() {
        let mut cli = empty_cli();
        cli.disks = Some("/dev/nvme0n1,/dev/nvme1n1".into());
        let p = planned_disks_from_cli(&cli);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn occupied_hint_requires_nuke_word_not_yes() {
        // Documented contract: YES is for final wipe; NUKE is for existing-OS gate
        assert!(!parse_nuke_confirm("YES"));
        assert!(parse_nuke_confirm("NUKE"));
    }
}
