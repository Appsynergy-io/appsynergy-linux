//! Thin process helpers. Every external tool call goes through here so steps
//! can report a named failure and we never silently ignore non-zero exits
//! unless the caller opts in.

use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn run(step: &str, program: impl AsRef<OsStr>, args: &[&str]) -> Result<()> {
    let program = program.as_ref();
    eprintln!("    $ {} {}", program.to_string_lossy(), args.join(" "));
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("[{step}] spawn {}", program.to_string_lossy()))?;
    if !status.success() {
        bail!(
            "[{step}] {} exited with {}",
            program.to_string_lossy(),
            status
        );
    }
    Ok(())
}

/// Like [`run`] but feed `stdin` bytes (e.g. password / keyfile contents).
pub fn run_stdin(step: &str, program: impl AsRef<OsStr>, args: &[&str], stdin: &[u8]) -> Result<()> {
    use std::io::Write;
    let program = program.as_ref();
    eprintln!("    $ {} {}  <stdin:{}B>", program.to_string_lossy(), args.join(" "), stdin.len());
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("[{step}] spawn {}", program.to_string_lossy()))?;
    {
        let mut sin = child.stdin.take().context("stdin")?;
        sin.write_all(stdin)
            .with_context(|| format!("[{step}] write stdin"))?;
    }
    let status = child
        .wait()
        .with_context(|| format!("[{step}] wait {}", program.to_string_lossy()))?;
    if !status.success() {
        bail!(
            "[{step}] {} exited with {}",
            program.to_string_lossy(),
            status
        );
    }
    Ok(())
}

pub fn run_ok(step: &str, program: impl AsRef<OsStr>, args: &[&str]) {
    if let Err(e) = run(step, program, args) {
        eprintln!("WARN [{step}]: {e}");
    }
}

pub fn output(step: &str, program: impl AsRef<OsStr>, args: &[&str]) -> Result<String> {
    let program = program.as_ref();
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("[{step}] spawn {}", program.to_string_lossy()))?;
    if !out.status.success() {
        bail!(
            "[{step}] {} failed: {}",
            program.to_string_lossy(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn which(bin: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {bin} >/dev/null 2>&1")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn need(bin: &str) -> Result<()> {
    if which(bin) {
        Ok(())
    } else {
        bail!("missing required command: {bin}");
    }
}

pub fn arch_chroot(mnt: &Path, shell: &str) -> Result<()> {
    run(
        "arch-chroot",
        "arch-chroot",
        &[
            mnt.to_str().unwrap_or("/mnt/appsynergy"),
            "bash",
            "-c",
            shell,
        ],
    )
}

pub fn arch_chroot_ok(mnt: &Path, shell: &str) {
    if let Err(e) = arch_chroot(mnt, shell) {
        eprintln!("WARN [arch-chroot]: {e}");
    }
}
