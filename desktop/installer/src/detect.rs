//! Hardware detection for **kernel installability** and TPM messaging.
//!
//! There is one kernel: `appsynergy-linux`, CachyOS's `linux-cachyos-server`
//! built with ThinLTO under our name, from upstream's unmodified config. Product
//! variant (desktop | server) selects packages and services, never the kernel.
//!
//! This module no longer chooses between per-CPU builds — there are none. What
//! it does instead is refuse hardware the single kernel cannot run: it is built
//! `GENERIC_V3`, so a CPU without the x86-64-v3 feature set will not boot it.
//! The old per-`-march=` packages made that unrepresentable; one package does
//! not, so the check moved here.

use std::fs;
use std::path::Path;

/// The only kernel package. Locked against `kernel/upstream/PIN` by
/// `adversarial_tests::installer_kernel_package_matches_the_pin`.
pub const KERNEL_PKG: &str = "appsynergy-linux";

/// Retired kernel packages. Kept solely so an upgrade path can find and remove
/// them — never installed, never selected.
pub const LEGACY_KERNEL_PKGS: &[&str] = &[
    "linux-appsynergy",
    "linux-appsynergy-server",
    "linux-appsynergy-server-skylake",
    "linux-appsynergy-server-tigerlake",
    "linux-appsynergy-server-ovh",
    "linux-appsynergy-server-nuc",
    "linux-cachyos-igpu",
];

/// Result of kernel selection for a chosen product variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelSelection {
    /// Human CPU model line.
    pub cpu_model: String,
    /// Short family label for logs (e.g. "Coffee Lake", "Tiger Lake").
    pub family_label: String,
    /// Package prefixes to install (headers resolved separately).
    /// Empty means this CPU cannot run the kernel we ship.
    pub pkg_prefixes: Vec<&'static str>,
    /// Why this mapping was chosen.
    pub reason: String,
}

/// Read `model name` from `/proc/cpuinfo` (first core).
pub fn read_cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| {
                    l.split_once(':')
                        .map(|(_, v)| v.trim().to_string())
                        .unwrap_or_default()
                })
        })
        .unwrap_or_default()
}

/// Read the flag list from `/proc/cpuinfo` (first core).
pub fn read_cpu_flags() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("flags"))
                .map(|l| {
                    l.split_once(':')
                        .map(|(_, v)| v.trim().to_string())
                        .unwrap_or_default()
                })
        })
        .unwrap_or_default()
}

/// The x86-64-v3 psABI level, tested against a `/proc/cpuinfo` flag list.
///
/// Checked as features rather than by model name: a name table would have to be
/// kept current forever and would guess wrong on VMs, which report the host CPU
/// model while masking features the guest does not actually have.
pub fn supports_x86_64_v3(flags: &str) -> bool {
    // The v3 additions over v2 that the kernel's GENERIC_V3 build actually emits.
    const NEEDED: &[&str] = &["avx", "avx2", "bmi1", "bmi2", "fma", "f16c", "movbe", "xsave"];
    let have: Vec<&str> = flags.split_whitespace().collect();
    // `abm` is reported as `lzcnt`-implying `abm` on AMD and as `lzcnt` via
    // `abm` on Intel; accept either spelling.
    let abm = have.contains(&"abm") || have.contains(&"lzcnt");
    NEEDED.iter().all(|n| have.contains(n)) && abm
}

/// Pick the kernel package for the operator-chosen variant + live CPU.
///
/// `variant_is_server` is accepted and deliberately unused for the kernel: both
/// variants run the same one. It stays in the signature because callers pass the
/// variant and a future divergence should be a compile error, not a silent one.
pub fn select_kernel_for_variant(
    _variant_is_server: bool,
    cpu_model: &str,
    cpu_flags: &str,
) -> KernelSelection {
    let family = family_label(cpu_model);

    if !supports_x86_64_v3(cpu_flags) {
        return KernelSelection {
            cpu_model: cpu_model.to_string(),
            family_label: family,
            pkg_prefixes: vec![],
            reason: format!(
                "CPU lacks the x86-64-v3 feature set that {KERNEL_PKG} is built for \
                 (needs AVX2/BMI2/FMA/F16C/MOVBE); cpu={cpu_model:?}"
            ),
        };
    }

    KernelSelection {
        cpu_model: cpu_model.to_string(),
        family_label: family,
        pkg_prefixes: vec![KERNEL_PKG],
        reason: format!("x86-64-v3 capable → {KERNEL_PKG}"),
    }
}

/// Short family label for banners (best-effort).
pub fn family_label(cpu_model: &str) -> String {
    let m = cpu_model.to_ascii_lowercase();
    if m.contains("1185g7") || m.contains("tiger lake") || m.contains("tigerlake") {
        return "Tiger Lake".into();
    }
    if m.contains("ice lake") || m.contains("icelake") {
        return "Ice Lake".into();
    }
    if m.contains("alder lake") || m.contains("12th gen") || m.contains("12900") {
        return "Alder Lake".into();
    }
    if m.contains("raptor lake") || m.contains("13th gen") || m.contains("14th gen") {
        return "Raptor Lake".into();
    }
    if m.contains("coffee lake") || m.contains("coffeelake") || m.contains("8th gen") || m.contains("9th gen")
    {
        return "Coffee Lake".into();
    }
    if m.contains("kaby lake") || m.contains("kabylake") || m.contains("7th gen") {
        return "Kaby Lake".into();
    }
    if m.contains("comet lake") || m.contains("10th gen") {
        return "Comet Lake".into();
    }
    if m.contains("skylake") || m.contains("6th gen") || m.contains("e3-1270") {
        return "Skylake".into();
    }
    if m.contains("11th gen") {
        return "11th gen".into();
    }
    if cpu_model.trim().is_empty() {
        return "unknown".into();
    }
    let t = cpu_model.trim();
    if t.len() > 48 {
        format!("{}…", &t[..45])
    } else {
        t.to_string()
    }
}

/// TPM device present (for auto-enroll when not --no-tpm).
pub fn tpm_present() -> bool {
    Path::new("/dev/tpm0").exists() || Path::new("/dev/tpmrm0").exists()
}

/// Live selection using `/proc/cpuinfo`.
pub fn select_kernel_live(variant_is_server: bool) -> KernelSelection {
    select_kernel_for_variant(variant_is_server, &read_cpu_model(), &read_cpu_flags())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flag list from the OVH appliance (Xeon E3-1270 v6), trimmed to what the
    /// v3 gate reads.
    const SKYLAKE_FLAGS: &str =
        "fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge sse sse2 ssse3 fma cx16 \
         sse4_1 sse4_2 movbe popcnt aes xsave avx f16c rdrand lahf_lm abm bmi1 avx2 bmi2";

    #[test]
    fn both_variants_get_the_one_kernel() {
        for server in [false, true] {
            let s = select_kernel_for_variant(
                server,
                "Intel(R) Xeon(R) CPU E3-1270 v6 @ 3.80GHz",
                SKYLAKE_FLAGS,
            );
            assert_eq!(s.pkg_prefixes, [KERNEL_PKG]);
        }
    }

    #[test]
    fn tigerlake_and_skylake_get_the_same_package() {
        let a = select_kernel_for_variant(true, "Intel(R) Xeon(R) CPU E3-1270 v6", SKYLAKE_FLAGS);
        let b = select_kernel_for_variant(
            true,
            "11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz",
            SKYLAKE_FLAGS,
        );
        assert_eq!(a.pkg_prefixes, b.pkg_prefixes);
    }

    #[test]
    fn pre_v3_cpu_is_refused_not_silently_given_an_unbootable_kernel() {
        // Sandy Bridge: has avx, lacks avx2/bmi2/fma.
        let sandy = "fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge sse sse2 \
                     ssse3 cx16 sse4_1 sse4_2 popcnt aes xsave avx lahf_lm";
        let s = select_kernel_for_variant(true, "Intel(R) Xeon(R) CPU E5-2670 0 @ 2.60GHz", sandy);
        assert!(s.pkg_prefixes.is_empty());
        assert!(s.reason.contains("x86-64-v3"));
    }

    #[test]
    fn v3_gate_reads_features_not_model_names() {
        assert!(supports_x86_64_v3(SKYLAKE_FLAGS));
        // Same model string, features masked as a hypervisor may present them.
        assert!(!supports_x86_64_v3("fpu sse sse2 avx popcnt"));
    }

    #[test]
    fn legacy_names_are_not_selectable() {
        let s = select_kernel_for_variant(true, "Intel(R) Xeon(R) CPU E3-1270 v6", SKYLAKE_FLAGS);
        for legacy in LEGACY_KERNEL_PKGS {
            assert!(!s.pkg_prefixes.contains(legacy), "{legacy} still selectable");
        }
    }
}
