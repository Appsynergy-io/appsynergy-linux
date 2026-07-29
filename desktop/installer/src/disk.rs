//! Disk layout planning for single-disk and dual-NVMe btrfs RAID1.
//! Pure helpers — no shell — so adversarial tests can cover failure modes.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// One physical disk's install roles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskMember {
    pub disk: PathBuf,
    pub efi_part: PathBuf,
    pub luks_part: PathBuf,
    /// dm-crypt mapper name (cryptroot | crypt0 | crypt1).
    pub cryptname: String,
}

impl DiskMember {
    pub fn mapper_path(&self) -> PathBuf {
        PathBuf::from(format!("/dev/mapper/{}", self.cryptname))
    }
}

/// Planned install layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskLayout {
    pub members: Vec<DiskMember>,
    /// true when two (or more) disks form btrfs RAID1.
    pub raid1: bool,
    pub efi_size: String,
    pub label: String,
}

impl DiskLayout {
    pub fn primary(&self) -> &DiskMember {
        &self.members[0]
    }

    pub fn is_raid1(&self) -> bool {
        self.raid1
    }

    pub fn mapper_paths(&self) -> Vec<PathBuf> {
        self.members.iter().map(|m| m.mapper_path()).collect()
    }
}

/// nvme0n1 / mmcblk0 / loop0 style → p1/p2; sda → 1/2.
/// `loop` included so loopback-device rehearsals of an install are faithful.
pub fn is_nvme_style(disk: &str) -> bool {
    disk.contains("nvme") || disk.contains("mmcblk") || disk.contains("loop")
}

pub fn partition_paths(disk: &Path) -> (PathBuf, PathBuf) {
    let s = disk.to_string_lossy();
    if is_nvme_style(&s) {
        (
            PathBuf::from(format!("{s}p1")),
            PathBuf::from(format!("{s}p2")),
        )
    } else {
        (PathBuf::from(format!("{s}1")), PathBuf::from(format!("{s}2")))
    }
}

/// Parse `APPSYNERGY_DISKS` / `--disks` list: comma or space separated.
pub fn parse_disks_list(s: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for part in s.split(|c: char| c == ',' || c.is_whitespace()) {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if !p.starts_with("/dev/") {
            bail!("disk path must start with /dev/: {p}");
        }
        out.push(PathBuf::from(p));
    }
    if out.is_empty() {
        bail!("empty disk list");
    }
    // dedupe preserve order
    let mut seen = std::collections::HashSet::new();
    out.retain(|d| seen.insert(d.clone()));
    Ok(out)
}

/// Build layout. Dual disks → RAID1 crypt0/crypt1; single → cryptname (default cryptroot).
pub fn plan_layout(
    disks: &[PathBuf],
    efi_size: &str,
    single_cryptname: &str,
    label: &str,
    force_raid1: bool,
) -> Result<DiskLayout> {
    if disks.is_empty() {
        bail!("at least one disk required");
    }
    if disks.len() > 2 {
        bail!("at most two disks supported (got {})", disks.len());
    }
    if disks.len() == 2 && disks[0] == disks[1] {
        bail!("both disks are the same path: {}", disks[0].display());
    }

    let raid1 = disks.len() == 2 || force_raid1;
    if force_raid1 && disks.len() < 2 {
        bail!("RAID1 requires two disks");
    }

    let mut members = Vec::new();
    for (i, disk) in disks.iter().enumerate() {
        let (efi, luks) = partition_paths(disk);
        let cryptname = if raid1 {
            format!("crypt{i}")
        } else {
            single_cryptname.to_string()
        };
        members.push(DiskMember {
            disk: disk.clone(),
            efi_part: efi,
            luks_part: luks,
            cryptname,
        });
    }

    Ok(DiskLayout {
        members,
        raid1,
        efi_size: efi_size.to_string(),
        label: label.to_string(),
    })
}

/// crypttab lines. `tpm` adds tpm2-device=auto.
pub fn render_crypttab(entries: &[(String, String, bool)]) -> String {
    // (cryptname, uuid, tpm)
    let mut out = String::new();
    for (name, uuid, tpm) in entries {
        let opts = if *tpm {
            "luks,discard,tpm2-device=auto,x-initrd.attach"
        } else {
            "luks,discard,x-initrd.attach"
        };
        out.push_str(&format!("{name} UUID={uuid} none {opts}\n"));
    }
    out
}

/// Kernel cmdline rd.luks.name fragments + root.
/// For RAID1: multiple rd.luks.name= and root by btrfs UUID when provided.
pub fn render_cmdline_luks(
    entries: &[(String, String)], // (cryptname, luks_uuid)
    root_spec: &str,             // UUID=… or /dev/mapper/crypt0
    extra: &str,
) -> String {
    let mut opts = String::new();
    for (name, uuid) in entries {
        opts.push_str(&format!("rd.luks.name={uuid}={name} "));
    }
    opts.push_str(&format!("root={root_spec} rootflags=subvol=@ rw zswap.enabled=0"));
    if !extra.is_empty() {
        opts.push(' ');
        opts.push_str(extra.trim());
    }
    opts
}

/// INSTALL-PROBLEMS #1: os-release must only be written to usr/lib; etc is symlink.
/// Returns (lib_path_rel, etc_symlink_target) for documentation/tests.
pub fn os_release_write_plan() -> (&'static str, &'static str) {
    ("usr/lib/os-release", "../usr/lib/os-release")
}

/// Pacstrap filter: branding/kernel packages must not be pacstrap'd (INSTALL-PROBLEMS #2).
pub fn should_skip_pacstrap_pkg(name: &str) -> bool {
    matches!(
        name,
        "appsynergy-branding"
            | "appsynergy-mirrorlist"
            | "linux-appsynergy"
            | "linux-appsynergy-headers"
            | "linux-appsynergy-server"
            | "linux-appsynergy-server-headers"
            | "linux-appsynergy-server-skylake"
            | "linux-appsynergy-server-skylake-headers"
            | "linux-appsynergy-server-tigerlake"
            | "linux-appsynergy-server-tigerlake-headers"
            | "linux-cachyos-igpu"
            | "linux-cachyos-igpu-headers"
    )
}

/// Password file: strip one trailing newline (INSTALL-PROBLEMS #3 batch).
pub fn strip_password_newline(mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.is_empty() {
        bail!("password empty");
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    if bytes.is_empty() {
        bail!("password empty after stripping newline");
    }
    Ok(bytes)
}

/// EFI partition number from path (nvme0n1p1 → 1, sda1 → 1).
pub fn efi_part_number(efi_part: &Path) -> u32 {
    efi_part
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
                .parse()
                .ok()
        })
        .unwrap_or(1)
}

/// btrfs mkfs args for layout (without device paths).
pub fn btrfs_mkfs_profile_args(raid1: bool) -> Vec<&'static str> {
    if raid1 {
        vec!["-d", "raid1", "-m", "raid1"]
    } else {
        vec![]
    }
}

/// Server dual-disk default subvolume set (includes @var for ops).
pub fn subvolume_names(server: bool) -> &'static [&'static str] {
    if server {
        &["@", "@home", "@var", "@log", "@cache", "@snapshots", "@srv"]
    } else {
        &["@", "@home", "@log", "@cache", "@snapshots"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvme_partition_paths() {
        let (e, l) = partition_paths(Path::new("/dev/nvme0n1"));
        assert_eq!(e, PathBuf::from("/dev/nvme0n1p1"));
        assert_eq!(l, PathBuf::from("/dev/nvme0n1p2"));
    }

    #[test]
    fn sda_partition_paths() {
        let (e, l) = partition_paths(Path::new("/dev/sda"));
        assert_eq!(e, PathBuf::from("/dev/sda1"));
        assert_eq!(l, PathBuf::from("/dev/sda2"));
    }

    #[test]
    fn dual_nvme_raid1_layout() {
        let disks = parse_disks_list("/dev/nvme0n1,/dev/nvme1n1").unwrap();
        let lay = plan_layout(&disks, "1G", "cryptroot", "appsynergy-server", false).unwrap();
        assert!(lay.raid1);
        assert_eq!(lay.members.len(), 2);
        assert_eq!(lay.members[0].cryptname, "crypt0");
        assert_eq!(lay.members[1].cryptname, "crypt1");
        assert_eq!(lay.members[0].efi_part, PathBuf::from("/dev/nvme0n1p1"));
        assert_eq!(lay.members[1].luks_part, PathBuf::from("/dev/nvme1n1p2"));
        assert_eq!(
            lay.mapper_paths(),
            vec![
                PathBuf::from("/dev/mapper/crypt0"),
                PathBuf::from("/dev/mapper/crypt1")
            ]
        );
    }

    #[test]
    fn single_disk_no_raid() {
        let disks = vec![PathBuf::from("/dev/nvme0n1")];
        let lay = plan_layout(&disks, "2G", "cryptroot", "appsynergy", false).unwrap();
        assert!(!lay.raid1);
        assert_eq!(lay.primary().cryptname, "cryptroot");
    }

    #[test]
    fn reject_same_disk_twice() {
        let disks = parse_disks_list("/dev/nvme0n1,/dev/nvme0n1").unwrap();
        // dedupe → one disk
        assert_eq!(disks.len(), 1);
    }

    #[test]
    fn reject_three_disks() {
        let disks = parse_disks_list("/dev/nvme0n1,/dev/nvme1n1,/dev/nvme2n1").unwrap();
        assert!(plan_layout(&disks, "1G", "cryptroot", "x", false).is_err());
    }

    #[test]
    fn crypttab_tpm_and_dual() {
        let t = render_crypttab(&[
            ("crypt0".into(), "aaa".into(), true),
            ("crypt1".into(), "bbb".into(), true),
        ]);
        assert!(t.contains("crypt0 UUID=aaa none luks,discard,tpm2-device=auto,x-initrd.attach"));
        assert!(t.contains("crypt1 UUID=bbb"));
        assert_eq!(t.lines().count(), 2);
    }

    #[test]
    fn cmdline_dual_luks_root_uuid() {
        let c = render_cmdline_luks(
            &[
                ("crypt0".into(), "u0".into()),
                ("crypt1".into(), "u1".into()),
            ],
            "UUID=btrfs-root",
            "preempt=voluntary ip=dhcp",
        );
        assert!(c.contains("rd.luks.name=u0=crypt0"));
        assert!(c.contains("rd.luks.name=u1=crypt1"));
        assert!(c.contains("root=UUID=btrfs-root rootflags=subvol=@"));
        assert!(c.contains("preempt=voluntary"));
    }

    #[test]
    fn os_release_plan_is_lib_not_etc_copy() {
        // INSTALL-PROBLEMS #1
        let (lib, link) = os_release_write_plan();
        assert_eq!(lib, "usr/lib/os-release");
        assert_eq!(link, "../usr/lib/os-release");
        assert!(!lib.starts_with("etc/"));
    }

    #[test]
    fn pacstrap_skip_branding_and_kernel() {
        // INSTALL-PROBLEMS #2
        assert!(should_skip_pacstrap_pkg("appsynergy-branding"));
        assert!(should_skip_pacstrap_pkg("appsynergy-mirrorlist"));
        assert!(should_skip_pacstrap_pkg("linux-appsynergy-server"));
        assert!(!should_skip_pacstrap_pkg("openssh"));
    }

    #[test]
    fn password_newline_strip() {
        // INSTALL-PROBLEMS #3
        assert_eq!(strip_password_newline(b"secret\n".to_vec()).unwrap(), b"secret");
        assert_eq!(strip_password_newline(b"secret\r\n".to_vec()).unwrap(), b"secret");
        assert_eq!(strip_password_newline(b"secret".to_vec()).unwrap(), b"secret");
        assert!(strip_password_newline(b"\n".to_vec()).is_err());
        assert!(strip_password_newline(vec![]).is_err());
    }

    #[test]
    fn btrfs_raid1_mkfs_flags() {
        assert_eq!(btrfs_mkfs_profile_args(true), vec!["-d", "raid1", "-m", "raid1"]);
        assert!(btrfs_mkfs_profile_args(false).is_empty());
    }

    #[test]
    fn efi_part_number_nvme() {
        assert_eq!(efi_part_number(Path::new("/dev/nvme0n1p1")), 1);
        assert_eq!(efi_part_number(Path::new("/dev/sda1")), 1);
    }

    #[test]
    fn parse_disks_requires_dev() {
        assert!(parse_disks_list("nvme0n1").is_err());
    }
}
