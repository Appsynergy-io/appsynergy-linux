//! Hardware detection for **kernel package selection** only.
//!
//! Product variant (desktop | server) is always the operator's choice.
//! This module maps live CPU → which local kernel package to install for that
//! variant, and reports TPM presence for auto-enroll messaging.
//!
//! Server packages today:
//! - `linux-appsynergy-server-skylake`  — -march=skylake (also Kaby/Coffee/Comet)
//! - `linux-appsynergy-server-tigerlake` — -march=tigerlake (11th gen / 1185G7)
//!
//! Desktop package today:
//! - `linux-appsynergy` — workstation (one package; CPU logged for clarity)

use std::fs;
use std::path::Path;

/// Host-max server kernel ISA family (matches package name suffix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerKernelFlavor {
    /// Skylake-class ISA: Skylake, Kaby Lake, Coffee Lake, Comet Lake, Cascade, Xeon E3 v5/v6.
    Skylake,
    /// Tiger Lake-class: Tiger Lake, Ice Lake, 11th-gen mobile (e.g. i7-1185G7).
    Tigerlake,
}

impl ServerKernelFlavor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skylake => "skylake",
            Self::Tigerlake => "tigerlake",
        }
    }

    /// Package name prefix (no version): `linux-appsynergy-server-{flavor}`.
    pub fn pkg_prefix(self) -> &'static str {
        match self {
            Self::Skylake => "linux-appsynergy-server-skylake",
            Self::Tigerlake => "linux-appsynergy-server-tigerlake",
        }
    }

    /// Legacy package prefixes that map to the same flavor.
    pub fn legacy_pkg_prefixes(self) -> &'static [&'static str] {
        match self {
            Self::Skylake => &["linux-appsynergy-server-ovh"],
            Self::Tigerlake => &["linux-appsynergy-server-nuc"],
        }
    }
}

impl std::fmt::Display for ServerKernelFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Result of CPU → kernel selection for a chosen product variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelSelection {
    /// Human CPU model line (lowercased for matching; display uses original-ish).
    pub cpu_model: String,
    /// Short family label for logs (e.g. "Coffee Lake", "Tiger Lake", "Alder Lake").
    pub family_label: String,
    /// Server host-max flavor when variant is server and we can map the CPU.
    pub server_flavor: Option<ServerKernelFlavor>,
    /// Package prefixes to install (kernel + matching headers resolved separately).
    /// Server: exactly one host-max prefix when mapped. Desktop: `linux-appsynergy`.
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

/// Classify a CPU model string into a server kernel flavor (pure; testable).
///
/// Maps microarchitectures that run correctly on the shipped `-march=` packages:
/// - **skylake pkg**: Skylake → Comet/Cascade (+ Kaby/Coffee) and Xeon E3 v5/v6
/// - **tigerlake pkg**: Ice/Tiger Lake and 11th-gen mobile
///
/// Returns `None` when no host-max server package is a sensible match (e.g. Alder Lake).
pub fn classify_server_flavor(cpu_model: &str) -> Option<(ServerKernelFlavor, &'static str)> {
    let m = cpu_model.to_ascii_lowercase();

    // --- Tiger Lake class (check before generic "lake" / gen numbers that overlap) ---
    if m.contains("1185g7")
        || m.contains("tiger lake")
        || m.contains("tigerlake")
        || m.contains("ice lake")
        || m.contains("icelake")
        || m.contains("11th gen")
    {
        return Some((ServerKernelFlavor::Tigerlake, "Tiger Lake / 11th-gen class"));
    }

    // --- Skylake class: explicit microarch names ---
    if m.contains("skylake") {
        return Some((ServerKernelFlavor::Skylake, "Skylake"));
    }
    if m.contains("kaby lake") || m.contains("kabylake") {
        return Some((ServerKernelFlavor::Skylake, "Kaby Lake → skylake package"));
    }
    if m.contains("coffee lake") || m.contains("coffeelake") {
        return Some((ServerKernelFlavor::Skylake, "Coffee Lake → skylake package"));
    }
    if m.contains("comet lake") || m.contains("cometlake") {
        return Some((ServerKernelFlavor::Skylake, "Comet Lake → skylake package"));
    }
    if m.contains("cascade lake") || m.contains("cascadelake") {
        return Some((ServerKernelFlavor::Skylake, "Cascade Lake → skylake package"));
    }

    // Xeon E3 v5/v6 / E3-12xx (OVH-class)
    if m.contains("e3-1270")
        || m.contains("e3-12")
        || (m.contains("xeon") && (m.contains("v5") || m.contains("v6")))
        || (m.contains("xeon") && m.contains("e3-"))
    {
        return Some((ServerKernelFlavor::Skylake, "Xeon E3 v5/v6 class → skylake package"));
    }

    // Intel core gen strings that map to skylake-era client (6–10th gen)
    // e.g. "8th Gen Intel(R) Core(TM) i7-8700"
    if m.contains("6th gen")
        || m.contains("7th gen")
        || m.contains("8th gen")
        || m.contains("9th gen")
        || m.contains("10th gen")
    {
        return Some((
            ServerKernelFlavor::Skylake,
            "6th–10th gen Core → skylake package",
        ));
    }

    // Model number heuristics (i7-6700, i5-8400, i9-9900K, i7-10700, …)
    if let Some(label) = client_model_skylake_era(&m) {
        return Some((ServerKernelFlavor::Skylake, label));
    }

    None
}

/// Intel client model numbers: i7-6700 / i7-8700 / i9-9900K / i7-10700 → gen 6–10.
/// 4-digit: first digit is gen. 5-digit 10xxx: 10th gen. 11xxx+ handled elsewhere.
fn client_model_skylake_era(m: &str) -> Option<&'static str> {
    // Prefer the i[3579]-NNNN token (avoid matching other hyphens in the line).
    let lower = m;
    let marker = ["i3-", "i5-", "i7-", "i9-"]
        .into_iter()
        .find_map(|p| lower.find(p).map(|i| i + p.len()))?;
    let digits: String = lower[marker..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() == 4 {
        let gen = digits.chars().next()?.to_digit(10)?;
        if (6..=9).contains(&gen) {
            return Some("Core model 6xxx–9xxx → skylake package");
        }
    } else if digits.len() >= 5 && digits.starts_with("10") {
        return Some("Core model 10xxx → skylake package");
    }
    None
}

/// Desktop package prefix (single package line today).
pub fn desktop_pkg_prefix() -> &'static str {
    "linux-appsynergy"
}

/// Pick kernel package prefix(es) for the operator-chosen variant + live CPU.
pub fn select_kernel_for_variant(variant_is_server: bool, cpu_model: &str) -> KernelSelection {
    let family = family_label(cpu_model);

    if !variant_is_server {
        return KernelSelection {
            cpu_model: cpu_model.to_string(),
            family_label: family,
            server_flavor: None,
            pkg_prefixes: vec![desktop_pkg_prefix()],
            reason: "desktop variant → linux-appsynergy".into(),
        };
    }

    match classify_server_flavor(cpu_model) {
        Some((flavor, why)) => KernelSelection {
            cpu_model: cpu_model.to_string(),
            family_label: family,
            server_flavor: Some(flavor),
            pkg_prefixes: vec![flavor.pkg_prefix()],
            reason: format!("server variant + CPU → {} ({why})", flavor.pkg_prefix()),
        },
        None => KernelSelection {
            cpu_model: cpu_model.to_string(),
            family_label: family,
            server_flavor: None,
            pkg_prefixes: vec![],
            reason: format!(
                "server variant but CPU not mapped to a host-max package \
                 (have skylake + tigerlake only); cpu={cpu_model:?}"
            ),
        },
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
    // keep a short slice of the model for display
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
    select_kernel_for_variant(variant_is_server, &read_cpu_model())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_always_linux_appsynergy() {
        let s = select_kernel_for_variant(false, "Intel(R) Xeon(R) CPU E3-1270 v6 @ 3.80GHz");
        assert_eq!(s.pkg_prefixes, ["linux-appsynergy"]);
        assert!(s.server_flavor.is_none());
    }

    #[test]
    fn server_xeon_e3_1270_v6_skylake() {
        let s = select_kernel_for_variant(true, "Intel(R) Xeon(R) CPU E3-1270 v6 @ 3.80GHz");
        assert_eq!(s.server_flavor, Some(ServerKernelFlavor::Skylake));
        assert_eq!(s.pkg_prefixes, ["linux-appsynergy-server-skylake"]);
    }

    #[test]
    fn server_kaby_lake_maps_skylake_pkg() {
        let s = select_kernel_for_variant(true, "Intel(R) Core(TM) i7-7700K CPU @ 4.20GHz");
        assert_eq!(s.server_flavor, Some(ServerKernelFlavor::Skylake));
        assert_eq!(s.pkg_prefixes, ["linux-appsynergy-server-skylake"]);
    }

    #[test]
    fn server_coffee_lake_maps_skylake_pkg() {
        let s = select_kernel_for_variant(true, "Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz");
        assert_eq!(s.server_flavor, Some(ServerKernelFlavor::Skylake));
        assert!(s.reason.to_ascii_lowercase().contains("coffee") || s.pkg_prefixes[0].contains("skylake"));
        // 8th gen / 8700
        let s2 = select_kernel_for_variant(true, "8th Gen Intel(R) Core(TM) i5-8400");
        assert_eq!(s2.server_flavor, Some(ServerKernelFlavor::Skylake));
    }

    #[test]
    fn server_tigerlake_1185g7() {
        let s = select_kernel_for_variant(
            true,
            "11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz",
        );
        assert_eq!(s.server_flavor, Some(ServerKernelFlavor::Tigerlake));
        assert_eq!(s.pkg_prefixes, ["linux-appsynergy-server-tigerlake"]);
    }

    #[test]
    fn server_alder_lake_unmapped() {
        // No host-max server package for Alder Lake yet
        let s = select_kernel_for_variant(
            true,
            "12th Gen Intel(R) Core(TM) i9-12900K",
        );
        assert!(s.server_flavor.is_none());
        assert!(s.pkg_prefixes.is_empty());
    }

    #[test]
    fn flavor_pkg_prefix_stable() {
        assert_eq!(
            ServerKernelFlavor::Skylake.pkg_prefix(),
            "linux-appsynergy-server-skylake"
        );
        assert_eq!(
            ServerKernelFlavor::Tigerlake.pkg_prefix(),
            "linux-appsynergy-server-tigerlake"
        );
    }
}
