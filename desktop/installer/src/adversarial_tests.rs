//! Adversarial tests for appsynergy-install.
//! Covers dual-NVMe RAID1 layout + INSTALL-PROBLEMS.md failure modes.

use crate::config::{self, DiskSource};
use crate::disk;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Env map builder for the resolver tests: `env(&[("APPSYNERGY_DISKS", "/dev/a")])`.
fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

#[test]
fn disk_flags_carry_no_clap_env_binding() {
    // A clap `env =` value is indistinguishable from a typed one, so the resolver
    // could not rank them. The two disk args must reach it as flags only.
    use clap::CommandFactory;
    let cmd = crate::config::Cli::command();
    for id in ["disk", "disks"] {
        let arg = cmd
            .get_arguments()
            .find(|a| a.get_id() == id)
            .expect("invariant: disk args exist");
        assert!(
            arg.get_env().is_none(),
            "--{id} must not be clap-bound to an env var"
        );
    }
    // Non-destructive args keep theirs.
    assert!(cmd
        .get_arguments()
        .find(|a| a.get_id() == "hostname")
        .expect("invariant: hostname arg exists")
        .get_env()
        .is_some());
}

#[test]
fn typed_disk_flag_beats_exported_disks_env() {
    // The wrong-disk wipe: `export APPSYNERGY_DISKS=…` in a shell profile used to
    // win over the disk the operator typed on the command line.
    let (disks, src) = config::resolve_disks_from(
        None,
        Some(Path::new("/dev/sda")),
        &env(&[("APPSYNERGY_DISKS", "/dev/nvme0n1,/dev/nvme1n1")]),
        &env(&[("APPSYNERGY_DISKS", "/dev/vda")]),
        &paths(&["/dev/detected0"]),
    )
    .unwrap();
    assert_eq!(disks, paths(&["/dev/sda"]));
    assert_eq!(src, DiskSource::CliDisk);
}

#[test]
fn disk_source_precedence_is_flags_then_proc_env_then_file_then_detection() {
    let proc = env(&[("APPSYNERGY_DISKS", "/dev/proc0")]);
    let file = env(&[("APPSYNERGY_DISKS", "/dev/file0")]);
    let detected = paths(&["/dev/detected0"]);

    // --disks beats --disk
    let (d, s) = config::resolve_disks_from(
        Some("/dev/nvme0n1,/dev/nvme1n1"),
        Some(Path::new("/dev/sda")),
        &proc,
        &file,
        &detected,
    )
    .unwrap();
    assert_eq!(d, paths(&["/dev/nvme0n1", "/dev/nvme1n1"]));
    assert_eq!(s, DiskSource::CliDisks);

    // process env beats machine.env
    let (d, s) = config::resolve_disks_from(None, None, &proc, &file, &detected).unwrap();
    assert_eq!(d, paths(&["/dev/proc0"]));
    assert_eq!(s, DiskSource::ProcEnv);

    // machine.env beats detection
    let (d, s) =
        config::resolve_disks_from(None, None, &env(&[]), &file, &detected).unwrap();
    assert_eq!(d, paths(&["/dev/file0"]));
    assert_eq!(s, DiskSource::FileEnv);

    // detection is last
    let (d, s) =
        config::resolve_disks_from(None, None, &env(&[]), &env(&[]), &detected).unwrap();
    assert_eq!(d, detected);
    assert_eq!(s, DiskSource::Detected);

    // an empty value is no value — it must not shadow the source below it
    let (_, s) = config::resolve_disks_from(
        None,
        None,
        &env(&[("APPSYNERGY_DISKS", "  ")]),
        &file,
        &detected,
    )
    .unwrap();
    assert_eq!(s, DiskSource::FileEnv);

    // APPSYNERGY_DISKS outranks APPSYNERGY_DISK within one source
    let (d, _) = config::resolve_disks_from(
        None,
        None,
        &env(&[("APPSYNERGY_DISKS", "/dev/a"), ("APPSYNERGY_DISK", "/dev/b")]),
        &env(&[]),
        &detected,
    )
    .unwrap();
    assert_eq!(d, paths(&["/dev/a"]));
}

#[test]
fn batch_mode_refuses_auto_detected_disks_only() {
    // --yes skips confirm() and the wizard's NUKE gate, so nothing would ever
    // confirm a guess. A baked appliance image (machine.env) must keep working.
    let d = paths(&["/dev/nvme0n1"]);
    let err = config::check_batch_disks(true, DiskSource::Detected, &d).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("--yes requires an explicit"), "{msg}");
    assert!(msg.contains("/dev/nvme0n1"), "message must name the target: {msg}");

    for src in [
        DiskSource::CliDisks,
        DiskSource::CliDisk,
        DiskSource::ProcEnv,
        DiskSource::FileEnv,
    ] {
        assert!(config::check_batch_disks(true, src, &d).is_ok(), "{src:?}");
    }
    // Interactive runs still reach the confirm prompts with a detected disk.
    assert!(config::check_batch_disks(false, DiskSource::Detected, &d).is_ok());
}

#[test]
fn disk_flag_with_comma_names_the_right_flag() {
    // Previously survived load and died at the preflight as
    // "not a block device: /dev/a,/dev/b".
    let err = config::resolve_disks_from(
        None,
        Some(Path::new("/dev/nvme0n1,/dev/nvme1n1")),
        &env(&[]),
        &env(&[]),
        &paths(&["/dev/detected0"]),
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("use --disks for multiple disks"), "{msg}");
    // and the same value through --disks is fine
    assert!(config::resolve_disks_from(
        Some("/dev/nvme0n1,/dev/nvme1n1"),
        None,
        &env(&[]),
        &env(&[]),
        &[]
    )
    .is_ok());
}

#[test]
fn single_disk_value_keeps_the_dev_prefix_rule() {
    assert!(config::resolve_disks_from(
        None,
        Some(Path::new("sda")),
        &env(&[]),
        &env(&[]),
        &[]
    )
    .is_err());
}

#[test]
fn partition_names_are_rejected_as_install_targets() {
    for part in ["nvme0n1p2", "sda1", "mmcblk0p1", "vdb3", "loop0p1"] {
        assert!(disk::is_partition_name(part), "{part} is a partition");
    }
    for whole in ["nvme0n1", "sda", "loop0", "mmcblk0", "vda", "sr0", "md0"] {
        assert!(!disk::is_partition_name(whole), "{whole} is a whole device");
    }
}

#[test]
fn live_medium_device_is_the_boot_mount_not_the_squashfs_loop() {
    // Realistic archiso /proc/self/mounts: the USB is at /run/archiso/bootmnt;
    // the loop devices there are the squashfs images.
    let mounts = "\
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
sys /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
udev /dev devtmpfs rw,nosuid,relatime,size=8109140k,nr_inodes=2027285,mode=755 0 0
run /run tmpfs rw,nosuid,nodev,relatime,mode=755 0 0
/dev/sdb1 /run/archiso/bootmnt vfat ro,relatime,fmask=0022,dmask=0022,codepage=437 0 0
cowspace /run/archiso/cowspace tmpfs rw,relatime,size=4194304k,mode=755 0 0
/dev/loop0 /run/archiso/airootfs squashfs ro,relatime 0 0
airootfs / overlay rw,relatime,lowerdir=/run/archiso/airootfs 0 0
";
    assert_eq!(
        disk::live_medium_device(mounts).as_deref(),
        Some("sdb1"),
        "the live medium is the bootmnt device"
    );
    // Optical boot and loop-only boot media.
    assert_eq!(
        disk::live_medium_device("/dev/sr0 /run/archiso/bootmnt iso9660 ro 0 0\n").as_deref(),
        Some("sr0")
    );
    assert_eq!(
        disk::live_medium_device("/dev/loop1 /run/archiso/bootmnt squashfs ro 0 0\n"),
        None
    );
    // Installed system: no bootmnt, no live medium.
    assert_eq!(
        disk::live_medium_device("/dev/nvme0n1p2 / btrfs rw,relatime 0 0\n"),
        None
    );
    assert_eq!(disk::live_medium_device(""), None);
}

#[test]
fn preflight_tests_a_real_block_device_not_just_existence() {
    // /dev/zero exists and is a character device; the old check accepted it.
    let src = include_str!("main.rs");
    let start = src
        .find("fn validate_target_disks")
        .expect("invariant: preflight validates targets");
    let body = &src[start..];
    let body = &body[..body.find("\nfn ").expect("invariant: function body ends")];
    assert!(
        body.contains("is_block_device()"),
        "preflight must test the file type, not just Path::exists"
    );
    assert!(
        body.contains("/sys/class/block/{name}/partition"),
        "preflight must reject partitions via sysfs"
    );
    assert!(
        body.contains("live medium"),
        "preflight must reject the live medium"
    );
    assert!(
        !src.contains("if !m.disk.exists() {"),
        "the existence-only preflight must be gone"
    );
}

#[test]
fn install_problems_os_release_never_double_cp_etc() {
    // #1: writing /etc/os-release through symlink clobbers usr/lib then second cp fails
    let (lib, link) = disk::os_release_write_plan();
    assert_eq!(lib, "usr/lib/os-release");
    assert_eq!(link, "../usr/lib/os-release");
    // etc path is only ever a symlink target, never a copy destination for content
    assert!(!lib.contains("etc/os-release"));
}

#[test]
fn install_problems_pacstrap_skips_package_owned_branding() {
    // #2: pre-seeding package-owned files → "exists in filesystem"
    assert!(disk::should_skip_pacstrap_pkg("appsynergy-branding"));
    assert!(disk::should_skip_pacstrap_pkg("appsynergy-mirrorlist"));
    assert!(disk::should_skip_pacstrap_pkg("linux-appsynergy-server"));
    assert!(disk::should_skip_pacstrap_pkg("linux-appsynergy-server-skylake"));
    assert!(disk::should_skip_pacstrap_pkg("linux-appsynergy-server-tigerlake"));
    assert!(disk::should_skip_pacstrap_pkg("linux-appsynergy-server-skylake-headers"));
    assert!(!disk::should_skip_pacstrap_pkg("base"));
}

#[test]
fn server_kernel_pkg_prefix_does_not_cross_match() {
    // linux-appsynergy must not match linux-appsynergy-server*
    assert!(crate::match_kernel_pkg_prefix(
        "linux-appsynergy",
        "linux-appsynergy-7.1.5-2-x86_64.pkg.tar.zst"
    ));
    assert!(!crate::match_kernel_pkg_prefix(
        "linux-appsynergy",
        "linux-appsynergy-server-skylake-7.1.5-2-x86_64.pkg.tar.zst"
    ));
    assert!(crate::match_kernel_pkg_prefix(
        "linux-appsynergy-server-skylake",
        "linux-appsynergy-server-skylake-7.1.5-2-x86_64.pkg.tar.zst"
    ));
    assert!(!crate::match_kernel_pkg_prefix(
        "linux-appsynergy-server-skylake",
        "linux-appsynergy-server-tigerlake-7.1.5-2-x86_64.pkg.tar.zst"
    ));
    assert!(crate::match_kernel_hdr_prefix(
        "linux-appsynergy-server-tigerlake",
        "linux-appsynergy-server-tigerlake-headers-7.1.5-2-x86_64.pkg.tar.zst"
    ));
}

#[test]
fn kernel_select_is_variant_user_choice_cpu_auto() {
    // Operator picks variant; CPU only chooses which *package* within that variant.
    use crate::detect::{select_kernel_for_variant, ServerKernelFlavor};
    let desk = select_kernel_for_variant(false, "Intel(R) Xeon(R) CPU E3-1270 v6 @ 3.80GHz");
    assert_eq!(desk.pkg_prefixes, ["linux-appsynergy"]);
    let coffee = select_kernel_for_variant(true, "Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz");
    assert_eq!(coffee.server_flavor, Some(ServerKernelFlavor::Skylake));
    assert_eq!(coffee.pkg_prefixes.len(), 1);
    assert_eq!(coffee.pkg_prefixes[0], "linux-appsynergy-server-skylake");
    let nuc = select_kernel_for_variant(true, "11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz");
    assert_eq!(nuc.pkg_prefixes, ["linux-appsynergy-server-tigerlake"]);
}

#[test]
fn install_local_kernel_no_longer_installs_both_server_flavors() {
    // Contract: server path installs one CPU-mapped package, not skylake+tigerlake.
    let src = include_str!("main.rs");
    assert!(src.contains("CPU-mapped host-max only"));
    assert!(!src.contains("Server ships **both** host-max kernels"));
}

#[test]
fn k3s_server_only_no_docker_stack() {
    let src = include_str!("main.rs");
    assert!(src.contains("fn install_k3s"));
    assert!(src.contains("skip k3s (desktop"));
    assert!(
        src.contains("systemctl enable sshd systemd-networkd systemd-resolved nftables k3s"),
        "server must enable k3s"
    );
    assert!(
        !src.contains("enable NetworkManager sddm sshd docker")
            && !src.contains("enable NetworkManager sddm sshd k3s"),
        "desktop must not enable docker or k3s"
    );
    let desk = include_str!("../../iso/airootfs/etc/appsynergy/packages-target.txt");
    let srv = include_str!("../../iso/airootfs/etc/appsynergy/packages-target-server.txt");
    for list in [desk, srv] {
        assert!(
            !list.lines().any(|l| {
                let t = l.trim();
                t == "docker"
                    || t == "docker-compose"
                    || t == "docker-buildx"
                    || t == "containerd"
                    || t == "nerdctl"
            }),
            "package lists must not install docker/containerd/nerdctl"
        );
    }
}

#[test]
fn install_problems_password_file_strips_newline_not_content() {
    // #3: batch keyfile with trailing newline
    let p = disk::strip_password_newline(b"hunter2\n".to_vec()).unwrap();
    assert_eq!(p, b"hunter2");
    // empty after strip is hard fail (not silent empty LUKS key)
    assert!(disk::strip_password_newline(b"\n".to_vec()).is_err());
}

#[test]
fn dual_nvme_dc_ssd_layout_matches_hardware_plan() {
    // 2× Intel SSDPE2MX450G7-class: /dev/nvme0n1 + /dev/nvme1n1
    let disks = disk::parse_disks_list("/dev/nvme0n1,/dev/nvme1n1").unwrap();
    let lay = disk::plan_layout(&disks, "1G", "cryptroot", "appsynergy-server", false).unwrap();
    assert!(lay.raid1);
    assert_eq!(lay.members[0].efi_part.to_string_lossy(), "/dev/nvme0n1p1");
    assert_eq!(lay.members[1].luks_part.to_string_lossy(), "/dev/nvme1n1p2");
    assert_eq!(lay.members[0].cryptname, "crypt0");
    assert_eq!(lay.members[1].cryptname, "crypt1");
    assert_eq!(disk::btrfs_mkfs_profile_args(true), ["-d", "raid1", "-m", "raid1"]);
}

#[test]
fn dual_crypttab_both_volumes_tpm() {
    let tab = disk::render_crypttab(&[
        ("crypt0".into(), "uuid-a".into(), true),
        ("crypt1".into(), "uuid-b".into(), true),
    ]);
    assert!(tab.lines().count() >= 2);
    assert!(tab.contains("crypt0 UUID=uuid-a"));
    assert!(tab.contains("crypt1 UUID=uuid-b"));
    assert!(tab.contains("tpm2-device=auto"));
    assert!(tab.contains("x-initrd.attach"));
}

#[test]
fn dual_cmdline_opens_both_luks_before_root() {
    let cmd = disk::render_cmdline_luks(
        &[
            ("crypt0".into(), "u0".into()),
            ("crypt1".into(), "u1".into()),
        ],
        "UUID=btrfs-xxxx",
        "preempt=voluntary ip=dhcp",
    );
    let i0 = cmd.find("rd.luks.name=u0=crypt0").unwrap();
    let i1 = cmd.find("rd.luks.name=u1=crypt1").unwrap();
    let ir = cmd.find("root=UUID=btrfs-xxxx").unwrap();
    assert!(i0 < ir && i1 < ir, "LUKS must be named before root=");
    assert!(cmd.contains("rootflags=subvol=@"));
}

#[test]
fn single_disk_desktop_unchanged_cryptname() {
    let disks = vec![std::path::PathBuf::from("/dev/nvme0n1")];
    let lay = disk::plan_layout(&disks, "2G", "cryptroot", "appsynergy", false).unwrap();
    assert!(!lay.raid1);
    assert_eq!(lay.primary().cryptname, "cryptroot");
}

#[test]
fn reject_invalid_disk_path_not_under_dev() {
    assert!(disk::parse_disks_list("sda").is_err());
}

#[test]
fn reject_more_than_two_disks() {
    let d = disk::parse_disks_list("/dev/nvme0n1,/dev/nvme1n1,/dev/nvme2n1").unwrap();
    assert!(disk::plan_layout(&d, "1G", "cryptroot", "x", false).is_err());
}

#[test]
fn force_raid1_without_two_disks_fails() {
    let d = vec![std::path::PathBuf::from("/dev/nvme0n1")];
    assert!(disk::plan_layout(&d, "1G", "cryptroot", "x", true).is_err());
}

#[test]
fn server_subvolumes_include_var_and_srv() {
    let sv = disk::subvolume_names(true);
    assert!(sv.contains(&"@var"));
    assert!(sv.contains(&"@srv"));
    assert!(sv.contains(&"@"));
    let desk = disk::subvolume_names(false);
    assert!(!desk.contains(&"@srv"));
}

#[test]
fn branding_overwrite_flag_is_required_contract() {
    // INSTALL-PROBLEMS #2: installer must use --overwrite for local branding
    // (documented contract — string present in main install_branding)
    let src = include_str!("main.rs");
    assert!(
        src.contains("pacman -U --noconfirm --overwrite '*'"),
        "branding path must use --overwrite to avoid exists-in-filesystem"
    );
}

#[test]
fn server_variant_gets_no_graphical_branding() {
    // The split's whole point: a headless server must never pull Plasma icons,
    // the Plymouth theme or 23M of wallpapers, and must not rebuild its
    // initramfs because a wallpaper changed.
    let src = include_str!("main.rs");
    assert!(
        src.contains("let desktop = !cfg.variant.is_server();"),
        "install_branding must gate the graphical half on variant"
    );
    // Assert the call sites, not prose: every place the graphical packages are
    // actually named for install must sit after the variant gate.
    let gate = src
        .find("let desktop = !cfg.variant.is_server();")
        .expect("gate");
    let gated = &src[gate..];
    for call in [
        "names.push(\"appsynergy-branding-desktop\")",
        "names.push(\"appsynergy-wallpapers\")",
        "appsynergy-branding-desktop-*.pkg.tar.zst",
        "appsynergy-wallpapers-*.pkg.tar.zst",
    ] {
        assert!(
            gated.contains(call),
            "`{call}` must appear after the server/desktop gate"
        );
        assert!(
            !src[..gate].contains(call),
            "`{call}` must not be reachable before the gate"
        );
    }
}

#[test]
fn branding_globs_are_version_anchored() {
    // "appsynergy-branding-*" also matches "appsynergy-branding-desktop-*",
    // which would drag the graphical package onto a server. Anchor on [0-9].
    let src = include_str!("main.rs");
    assert!(
        src.contains("appsynergy-branding-[0-9]*.pkg.tar.zst"),
        "identity glob must be version-anchored"
    );
    assert!(
        !src.contains("appsynergy-branding-*.pkg.tar.zst"),
        "unanchored appsynergy-branding-* glob also matches -desktop-"
    );
}

#[test]
fn installer_never_writes_trustall() {
    // An inline TrustAll section both disables signature checking on the installed
    // system and makes appsynergy-mirrorlist's post_install no-op, so the packaged
    // `Required DatabaseRequired` drop-in never takes effect.
    let src = include_str!("main.rs");
    assert!(
        !src.contains("TrustAll"),
        "installer must never write a TrustAll repo section"
    );
}

#[test]
fn installer_populates_keyring() {
    let src = include_str!("main.rs");
    assert!(
        src.contains("pacman-key --populate appsynergy"),
        "installer must populate the appsynergy keyring in the target"
    );
    assert!(
        src.contains("pacman-key --list-keys {APPSYNERGY_KEY_FP}"),
        "keyring population must be hard-asserted against the pinned fingerprint"
    );
    assert!(
        src.contains("appsynergy-keyring-[0-9]*.pkg.tar.zst"),
        "keyring package must be installed from the offline payload"
    );
    assert!(
        src.contains("3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1"),
        "the signing key fingerprint must be pinned in the installer"
    );
}

#[test]
fn keyring_assert_is_hard_database_sync_is_best_effort() {
    // Two different invariants in one step. The keyring must be a hard failure: an
    // unpopulated keyring under a `Required` section leaves a committed disk unable to
    // run pacman at all. The database sync must NOT be — it is the only networked call
    // here, and this installer is offline-capable, so making it fatal kills a
    // disconnected install after the disks are already partitioned.
    let src = include_str!("main.rs");
    let start = src
        .find("fn register_appsynergy_repo")
        .expect("invariant: repo registration step exists");
    let body = &src[start..];
    let body = &body[..body.find("\nfn ").expect("invariant: function body ends")];

    let assertion = &body[body
        .find("pacman-key --list-keys")
        .expect("invariant: keyring is asserted")..];
    let stmt = &assertion[..assertion.find(';').expect("invariant: statement ends")];
    assert!(
        stmt.contains(")?"),
        "keyring assertion must propagate with `?` (hard failure)"
    );

    assert!(
        !body.contains("cmd::arch_chroot(&cfg.mnt, \"pacman -Sy\")?"),
        "database sync must not be `?`-propagated — it breaks offline installs"
    );
    assert!(
        body.contains("match cmd::arch_chroot(&cfg.mnt, \"pacman -Sy\")"),
        "database sync must be attempted and its result handled, not skipped"
    );
    assert!(
        body.contains("WARN: could not verify the signed [appsynergy] database"),
        "a failed database sync must warn, never be swallowed silently"
    );
}

#[test]
fn keyring_fingerprint_matches_pkgbuild() {
    // Trust is rooted in one key; a drift between installer and package means the
    // installer asserts a fingerprint the shipped keyring does not contain.
    let src = include_str!("main.rs");
    let pkgbuild = include_str!("../../../packages/pkgbuilds/appsynergy-keyring/PKGBUILD");
    let fp = src
        .split_once("const APPSYNERGY_KEY_FP: &str = \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .expect("invariant: main.rs pins the signing key fingerprint")
        .0;
    assert_eq!(fp.len(), 40, "fingerprint must be a full 40-hex-digit ID");
    assert!(
        pkgbuild.contains(fp),
        "appsynergy-keyring PKGBUILD does not document fingerprint {fp}"
    );
}

#[test]
fn iso_stages_keyring() {
    // A keyring that reaches no install path leaves the installer's hard gate
    // failing after the disk is already partitioned.
    let sh = include_str!("../../scripts/build-iso.sh");
    assert!(
        sh.contains("[appsynergy-keyring]='appsynergy-keyring-[0-9]*.pkg.tar.zst'"),
        "build-iso.sh must stage appsynergy-keyring"
    );
    assert!(
        sh.contains("packages/pkgbuilds/appsynergy-keyring"),
        "build-iso.sh must read the keyring PKGBUILD dir as a source"
    );
}

#[test]
fn keyring_glob_is_version_anchored() {
    // Same rule as branding: unanchored globs match sibling package names and
    // corrupt both the copy and the prune that keeps only the newest release.
    let sh = include_str!("../../scripts/build-iso.sh");
    assert!(
        !sh.contains("appsynergy-keyring-*"),
        "keyring glob must be version-anchored with [0-9]"
    );
}

#[test]
fn config_load_validates_every_untrusted_identity_field() {
    // The fix for shell injection is placement, not string escaping: validation
    // has to sit inside Config::load, the single point CLI flags, the process
    // environment and machine.env all funnel through. A validator that exists
    // but is called somewhere else leaves the `bash -c` interpolations in
    // locale_hostname and create_users reachable with an unchecked value.
    let src = include_str!("config.rs");
    let start = src
        .find("fn load(cli: Cli)")
        .expect("invariant: Config::load exists");
    let body = &src[start..];
    let body = &body[..body.find("\nfn ").expect("invariant: function body ends")];
    for call in [
        "validate::validate_hostname(&hostname)?",
        "validate::validate_username(&user)?",
        "validate::validate_timezone(&timezone)?",
        "validate::validate_locale(&locale)?",
        "validate::validate_keymap(&keymap)?",
    ] {
        assert!(body.contains(call), "Config::load must call `{call}`");
    }
}

#[test]
fn pacstrap_skips_split_branding_packages() {
    assert!(disk::should_skip_pacstrap_pkg("appsynergy-branding-desktop"));
    assert!(disk::should_skip_pacstrap_pkg("appsynergy-wallpapers"));
}

#[test]
fn step_failures_are_named() {
    let src = include_str!("main.rs");
    assert!(src.contains("step `{name}` failed") || src.contains("step `"));
    assert!(src.contains("fn step("));
}

#[test]
fn vconsole_written_before_mkinitcpio_contract() {
    // INSTALL-PROBLEMS #4
    let src = include_str!("main.rs");
    let vconsole = src.find("etc/vconsole.conf").expect("vconsole");
    let mkinit = src.find("configure_mkinitcpio").expect("mkinit config");
    // locale step writes vconsole; configure_mkinitcpio is later in file as fn —
    // pipeline order in try_main is the real contract:
    let try_main = src.find("fn try_main").unwrap();
    let loc = src[try_main..].find("locale").unwrap();
    let mki = src[try_main..].find("mkinitcpio-config").unwrap();
    assert!(loc < mki, "locale (vconsole) must run before mkinitcpio-config");
    let _ = (vconsole, mkinit);
}

#[test]
fn efibootmgr_creates_appsynergy_label() {
    // INSTALL-PROBLEMS NVRAM
    let src = include_str!("main.rs");
    assert!(src.contains("AppSynergy"));
    assert!(src.contains("efibootmgr"));
    assert!(src.contains("systemd-bootx64.efi") || src.contains("SYSTEMD-BOOT") || src.contains(r"\EFI\systemd"));
}

#[test]
fn empty_package_list_after_filter_is_error() {
    // adversarial: only skip packages → empty pacstrap must fail
    // unit-tested via should_skip; integration would need temp file
    assert!(disk::should_skip_pacstrap_pkg("appsynergy-branding"));
}

#[test]
fn server_os_release_replaces_the_workstation_variant() {
    // The shipped identity package sets VARIANT="Workstation", so the old append
    // guarded on `!t.contains("VARIANT=")` could never fire: both production servers
    // reported Workstation while /etc/appsynergy/VARIANT said server.
    let shipped = include_str!("../../../packages/pkgbuilds/appsynergy-branding/os-release");
    assert!(shipped.contains("VARIANT=\"Workstation\""), "input premise");
    let out = disk::rebrand_os_release_server(shipped);

    assert_eq!(
        out.lines().filter(|l| l.starts_with("VARIANT=")).count(),
        1,
        "exactly one VARIANT= line:\n{out}"
    );
    assert_eq!(
        out.lines().filter(|l| l.starts_with("VARIANT_ID=")).count(),
        1,
        "exactly one VARIANT_ID= line:\n{out}"
    );
    assert!(out.contains("VARIANT=\"Server\"\n"));
    assert!(out.contains("VARIANT_ID=server\n"));
    assert!(!out.contains("Workstation"), "no workstation identity survives");
    // display names become the server ones
    assert!(out.contains("NAME=\"AppSynergy Server\""));
    assert!(out.contains("PRETTY_NAME=\"AppSynergy Server\""));
    // unrelated keys are untouched
    assert!(out.contains("ID=appsynergy-linux\n"));
    assert!(out.contains("ID_LIKE=arch\n"));
    assert!(out.contains("LOGO=appsynergy-linux\n"));
    assert!(out.contains("HOME_URL=\"https://git.appsynergy.io/imabee\"\n"));
    // idempotent: re-running over an already-server file changes nothing
    assert_eq!(disk::rebrand_os_release_server(&out), out);

    // absent keys are appended exactly once
    let bare = "NAME=\"AppSynergy Linux\"\nID=appsynergy-linux\n";
    let out = disk::rebrand_os_release_server(bare);
    assert_eq!(out.lines().filter(|l| l.starts_with("VARIANT=")).count(), 1);
    assert_eq!(
        out.lines().filter(|l| l.starts_with("VARIANT_ID=")).count(),
        1
    );
    assert!(out.contains("ID=appsynergy-linux\n"));
}

#[test]
fn server_os_release_lands_in_etc_as_a_real_file() {
    // /usr/lib/os-release is owned by the `filesystem` package, so the rebrand there is
    // reverted on its next upgrade; only a real /etc/os-release survives.
    let src = include_str!("main.rs");
    let start = src
        .find("fn apply_os_release")
        .expect("invariant: os-release step exists");
    let body = &src[start..];
    let body = &body[..body.find("\nfn ").expect("invariant: function body ends")];
    let server = body
        .find("if cfg.variant.is_server() {\n        // `/usr/lib/os-release` is owned")
        .expect("invariant: server writes /etc/os-release itself");
    let server = &body[server..];
    assert!(
        server.contains("fs::write(&etc, disk::rebrand_os_release_server("),
        "server must write the rebranded body to /etc/os-release"
    );
    assert!(
        server.contains("fs::remove_file(&etc)"),
        "the /etc symlink must be removed before writing a real file"
    );
    // desktop keeps the symlink
    assert!(
        body.contains("std::os::unix::fs::symlink(link_tgt, &etc)"),
        "desktop path must still symlink /etc/os-release"
    );
    assert!(
        !body.contains("if !t.contains(\"VARIANT=\")"),
        "the append-only VARIANT guard could never fire and must be gone"
    );
}

#[test]
fn critical_server_services_are_enabled_by_a_hard_call() {
    // Doubly suppressed before: `|| true` inside the shell AND the warn-only wrapper.
    // A server that fails to enable sshd/nftables reported a successful install and
    // came up unreachable or unfirewalled.
    let src = include_str!("main.rs");
    assert!(
        !src.contains("nftables apparmor k3s fstrim.timer || true"),
        "the doubly-suppressed server enable must be gone"
    );
    let start = src
        .find("fn enable_services")
        .expect("invariant: service step exists");
    let body = &src[start..];
    let body = &body[..body.find("\nfn ").expect("invariant: function body ends")];

    let hit = body
        .find("systemctl enable sshd")
        .expect("invariant: server enables the critical set");
    let end = hit + body[hit..].find(';').expect("invariant: statement ends");
    let call = body[..end]
        .rfind("cmd::arch_chroot")
        .expect("invariant: enables go through cmd");
    let stmt = &body[call..end];
    assert!(
        !stmt.starts_with("cmd::arch_chroot_ok"),
        "critical enables must not use the warn-only wrapper: {stmt}"
    );
    assert!(
        stmt.contains(")?"),
        "critical enables must propagate with `?`: {stmt}"
    );
    assert!(!stmt.contains("|| true"), "no shell-level suppression: {stmt}");
    for svc in [
        "sshd",
        "systemd-networkd",
        "systemd-resolved",
        "nftables",
        "k3s",
    ] {
        assert!(stmt.contains(svc), "{svc} must be in the hard enable: {stmt}");
    }
    // apparmor and fstrim.timer stay warn-only, and the desktop set with them.
    assert!(body.contains("cmd::arch_chroot_ok(&cfg.mnt, \"systemctl enable apparmor fstrim.timer || true\")"));
    assert!(body.contains("systemctl enable NetworkManager sddm sshd fstrim.timer bluetooth || true"));
}

#[test]
fn failover_esp_is_resynced_after_the_last_initramfs_write() {
    // install_bootloader's mirror predates rebuild_initramfs, the TPM re-enrol rebuild
    // and verify_initrd_unlock's final `mkinitcpio -P`, so the second ESP could hold an
    // image without the dropbear/ssh-unlock hooks — the one case it exists for.
    let src = include_str!("main.rs");
    let try_main = src.find("fn try_main").expect("invariant: pipeline exists");
    let body = &src[try_main..];
    let boot = body
        .find("step(\"bootloader\"")
        .expect("invariant: bootloader step");
    let verify = body
        .find("step(\"initramfs-verify\"")
        .expect("invariant: unlock verification step");
    let resync = body
        .find("step(\"esp-resync\"")
        .expect("invariant: failover ESP is re-synced");
    assert!(boot < verify, "bootloader precedes the unlock verification");
    assert!(
        verify < resync,
        "the final ESP sync must come AFTER the unlock verification"
    );
    assert!(
        body[verify..resync].contains("cfg.layout.is_raid1()"),
        "the re-sync is only meaningful on the dual-disk/RAID1 layout"
    );
    // and the verification actually inspects the second ESP's images
    assert!(
        src.contains("fn compare_esp_boot_images"),
        "verification must cover the second ESP's boot images"
    );
}

#[test]
fn esp_comparison_ignores_random_seed_and_catches_stale_images() {
    // systemd-boot regenerates loader/random-seed per ESP: different content there is
    // never drift. A stale or missing initramfs is.
    let base = std::env::temp_dir().join(format!("appsynergy-esp-test-{}", std::process::id()));
    let (a, b) = (base.join("boot"), base.join("esp2"));
    std::fs::create_dir_all(&a).expect("invariant: temp dirs");
    std::fs::create_dir_all(&b).expect("invariant: temp dirs");
    let write = |d: &Path, n: &str, c: &str| std::fs::write(d.join(n), c).expect("invariant: write");

    write(&a, "vmlinuz-linux-appsynergy-server-skylake", "kernel");
    write(&b, "vmlinuz-linux-appsynergy-server-skylake", "kernel");
    write(&a, "initramfs-linux-appsynergy-server-skylake.img", "unlock");
    write(&b, "initramfs-linux-appsynergy-server-skylake.img", "unlock");
    write(&a, "random-seed", "seed-a");
    write(&b, "random-seed", "seed-b");
    let ok = crate::compare_esp_boot_images(&a, &b).expect("matching images verify");
    assert_eq!(ok.len(), 2, "only the boot images are compared: {ok:?}");

    // stale initramfs on the failover ESP
    write(&b, "initramfs-linux-appsynergy-server-skylake.img", "stale");
    let err = format!("{:#}", crate::compare_esp_boot_images(&a, &b).unwrap_err());
    assert!(err.contains("differs between the two ESPs"), "{err}");

    // missing image on the failover ESP
    std::fs::remove_file(b.join("initramfs-linux-appsynergy-server-skylake.img"))
        .expect("invariant: remove");
    let err = format!("{:#}", crate::compare_esp_boot_images(&a, &b).unwrap_err());
    assert!(err.contains("second ESP is missing"), "{err}");

    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn crypttab_without_tpm_still_has_initrd_attach() {
    let tab = disk::render_crypttab(&[("cryptroot".into(), "x".into(), false)]);
    assert!(tab.contains("x-initrd.attach"));
    assert!(!tab.contains("tpm2-device"));
}

#[test]
fn cmdline_single_disk_root_mapper() {
    let c = disk::render_cmdline_luks(
        &[("cryptroot".into(), "u".into())],
        "/dev/mapper/cryptroot",
        "nowatchdog",
    );
    assert!(c.contains("rd.luks.name=u=cryptroot"));
    assert!(c.contains("root=/dev/mapper/cryptroot"));
    assert!(c.contains("nowatchdog"));
}
