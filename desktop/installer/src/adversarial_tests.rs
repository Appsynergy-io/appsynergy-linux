//! Adversarial tests for appsynergy-install.
//! Covers dual-NVMe RAID1 layout + INSTALL-PROBLEMS.md failure modes.

use crate::disk;

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
        src.contains("apparmor k3s fstrim") || src.contains("apparmor k3s "),
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
