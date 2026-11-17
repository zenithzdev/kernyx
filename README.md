# KERNYX

[![CI](https://github.com/zenithzcloud/kernyx/actions/workflows/ci.yml/badge.svg)](https://github.com/zenithzcloud/kernyx/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**KERNYX** is an adaptive Linux performance optimization engine.

CLI command: `kernyx`

KERNYX detects real hardware and system configuration, calculates appropriate
kernel-level optimizations, presents the proposed changes, applies authorized
changes transactionally, verifies the results and supports rollback.

## Install

### Debian / Ubuntu

```bash
sudo apt update
sudo apt install kernyx
```

### From source

```bash
cargo build --release
sudo install -m 755 target/release/kernyx /usr/bin/kernyx
```

### From the included package

```bash
sudo dpkg -i dist/kernyx_2.0.0_amd64.deb
```

## Usage

```bash
kernyx
kernyx detect
kernyx analyze
kernyx optimize
kernyx optimize --profile vps
kernyx optimize --dry-run
kernyx status
kernyx report
kernyx doctor
kernyx rollback
kernyx history
kernyx config show
kernyx config validate
kernyx version
```

## Safety

- No SSH, firewall, boot configuration or XDP/eBPF changes.
- Transactional apply with backups and rollback.
- Idempotent execution.
- Unsupported settings are skipped.
- No fake performance claims.

## Repository layout

- `src/`, `Cargo.toml` — KERNYX Rust implementation
- `lib/`, `bin/ahopt`, `tests/` — previous Bash implementation retained for reference
- `.github/workflows/ci.yml` — CI pipeline

## Build

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo build --release
```

## License

MIT
