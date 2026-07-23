# appsynergy-install (Rust)

Destructive full-disk installer for the live USB. Replaces the former bash
script. Built into the ISO by `scripts/build-iso.sh`.

```bash
# host build
cargo build --release
sudo ./target/release/appsynergy-install --help

# batch (same password for LUKS + root + imma; no trailing newline preferred)
printf '%s' 'secret' > /tmp/appsynergy-key
sudo ./target/release/appsynergy-install --yes --password-file /tmp/appsynergy-key
shred -u /tmp/appsynergy-key
```

See `../README.md` and repo root install notes for package layout.
