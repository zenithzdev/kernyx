# KERNYX

[![CI](https://github.com/zenithzcloud/kernyx/actions/workflows/ci.yml/badge.svg)](https://github.com/zenithzcloud/kernyx/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**KERNYX** — Adaptive Linux Performance Optimization Engine.

KERNYX is a Rust Linux infrastructure utility that combines hardware detection,
genuine diagnostics, hardware-aware optimization, transactional apply/rollback,
and system-health reporting in a single `kernyx` command.

It does not ship generic "magic" values. Every setting is calculated from the
detected CPU, memory, storage, network, kernel features, virtualization and
cloud environment.

## Contents

- [Installation](#installation)
- [APT Repository](#apt-repository)
- [Quick start](#quick-start)
- [Full command reference](#full-command-reference)
- [Global options](#global-options)
- [Optimization workflow](#optimization-workflow)
- [Bundled upstream tools](#bundled-upstream-tools)
- [Safety model](#safety-model)
- [Persistence and rollback](#persistence-and-rollback)
- [Supported systems](#supported-systems)
- [Architecture](#architecture)
- [Build from source](#build-from-source)
- [CI and releases](#ci-and-releases)
- [Troubleshooting](#troubleshooting)
- [License and credits](#license-and-credits)

---

## Installation

### First time — one command

```bash
curl -fsSL https://zenithzdev.github.io/kernyx-apt/install.sh | sudo bash
```

The installer:
- detects distribution, version and CPU architecture
- verifies the system uses APT
- installs missing prerequisites (`ca-certificates`, `curl`, `gpg`)
- downloads the public GPG key and verifies its pinned fingerprint
- configures the signed APT repository with `signed-by=`
- runs `apt update` and installs KERNYX
- prints the installed version

### Normal APT usage

```bash
sudo apt install kernyx
sudo apt update && sudo apt upgrade
sudo apt remove kernyx
```

The repository setup can be removed separately by deleting:
```bash
sudo rm /etc/apt/sources.list.d/kernyx.list
sudo rm /etc/apt/keyrings/kernyx.gpg
```

---

## APT Repository

- Source repo: `https://github.com/zenithzdev/kernyx-apt`
- Public URL: `https://zenithzdev.github.io/kernyx-apt/`
- Public key: `kernyx.asc`
- Pinned fingerprint: `864FE821DCAF96A4B8F9F71EFB1D9A2606C3F536`
- Metadata is GPG-signed (`InRelease`, `Release.gpg`)
- No `trusted=yes`, no deprecated `apt-key`
- SHA256 checksums published with releases
- Multiple versions preserved

---

## Quick start

```bash
kernyx                          # concise system overview
kernyx detect                   # hardware detection
kernyx analyze                  # diagnostics + bottlenecks
kernyx optimize --dry-run       # simulate every change safely
kernyx optimize                 # apply (explicit authorization implied by user action)
kernyx status                   # show current state
kernyx report                   # show transaction report
kernyx rollback                 # undo the last transaction
kernyx fetch                    # real fastfetch system info
kernyx monitor                  # real btop monitoring UI
kernyx benchmark -- 'sleep 0.1' # hyperfine statistical benchmark
```

---

## Full command reference

| Command | Short alias | Description |
|---|---|---|
| `kernyx` | — | Default overview and next actions |
| `kernyx fetch` | `f` | Real Fastfetch system information |
| `kernyx monitor` | `m`, `mon` | Real btop monitoring interface (TTY required) |
| `kernyx detect` | `d`, `det` | Detect CPU, memory, storage, network, OS/cloud |
| `kernyx analyze` | `a`, `an` | Diagnostics: bottlenecks, health issues, conflicts |
| `kernyx optimize` | `o`, `opt` | Apply calculated optimizations transactionally |
| `kernyx status` | `s`, `st` | Transaction and persistence state |
| `kernyx report` | `r`, `rep` | Latest transaction report |
| `kernyx doctor` | `doc`, `dr` | Installation/compatibility diagnostics |
| `kernyx health` | `hl` | Server health summary |
| `kernyx process` | `p`, `ps`, `proc` | Top processes by CPU |
| `kernyx network` | `n`, `net` | Interface counters, routing, DNS, connections |
| `kernyx storage` | `stg`, `disk` | Disks, mounts and capacity |
| `kernyx memory` | `mem` | RAM, swap, buffers, cache |
| `kernyx cpu` | `cpuinfo` | CPU model, cores, frequency, load |
| `kernyx services` | `svc` | Failed service report |
| `kernyx security` | `sec` | Read-only security snapshot |
| `kernyx benchmark -- <cmd>` | `bench`, `bm` | Hyperfine statistical benchmark |
| `kernyx backup [LABEL]` | `backup` | Configuration snapshot |
| `kernyx backups [ID]` | `backups` | List/show snapshots |
| `kernyx rollback [--tx ID]` | `rb`, `roll` | Roll back a transaction |
| `kernyx history` | `hist` | Transaction history |
| `kernyx config show` | `c show`, `cfg show` | Effective settings |
| `kernyx config validate` | `c validate` | Validate configuration |
| `kernyx version` | `v`, `ver` | Version information |
| `kernyx update` | `up` | Update instructions |
| `kernyx uninstall` | `un` | Uninstall instructions |

Examples:
```bash
kernyx o --dry-run
kernyx o --profile vps
kernyx o --profile dedicated
kernyx o --profile proxmox
kernyx o --profile game-server
kernyx rb --tx <ID>
kernyx bench -- 'grep -R TODO .'
```

---

## Global options

| Option | Meaning |
|---|---|
| `-n`, `--dry-run` | Simulate only; write nothing |
| `-y`, `--yes` | Explicit non-interactive confirmation |
| `--json` | Machine-readable JSON output |
| `--quiet` | Suppress informational output |
| `--verbose` | Extra detail |
| `--profile NAME` | `automatic`, `vps`, `dedicated`, `proxmox`, `game-server` |
| `--no-imds` | Skip cloud metadata lookup |
| `-h`, `--help` | Help |
| `-V`, `--version` | Version |

---

## Optimization workflow

```
DETECT -> ANALYZE -> CALCULATE -> VALIDATE -> SHOW CHANGES -> APPLY -> VERIFY -> REPORT
```

1. `kernyx detect` — collect real hardware facts
2. `kernyx analyze` — find actual bottlenecks and misconfigurations
3. `kernyx optimize --dry-run` — see exactly what would change
4. `kernyx backup before` — snapshot managed configuration
5. `kernyx optimize` — apply under a transaction
6. `kernyx status` / `kernyx report` — verify and review
7. `kernyx rollback --tx <ID>` — undo if needed

Optimizations are calculated from detected hardware:
- CPU topology, SMT, cgroup limits, governor availability
- RAM size, swap, zram, memory pressure
- Storage class (NVMe/SSD/HDD/EBS/virtio)
- NIC speed, queues, driver capabilities
- Kernel feature availability (BBR/FQ, etc.)

No blind maximums. No hardcoded host-specific values.

---

## Bundled upstream tools

KERNYX packages include real upstream binaries; users never install them separately.

| Feature | Upstream | Version | Command |
|---|---|---|---|
| Monitoring UI | btop | 1.4.8 | `kernyx monitor` |
| System information | fastfetch | 2.71.0 | `kernyx fetch` |
| Benchmarking | hyperfine | 2.0.0 | `kernyx benchmark` |

Upstream licenses are preserved in `third_party/` and `vendor/btop/`.

---

## Safety model

KERNYX never automatically modifies:
- CPU mitigations, C-states, firmware/BIOS settings
- Guest VM/LXC definitions, storage pools, cluster config
- `/etc/fstab`, mount options, partitions, RAID, filesystems
- Bridges, bonds, VLANs, XDP/eBPF programs
- SSH access, firewall rules, boot configuration

Unsupported settings are skipped. ZFS/RPS/security tuning is opt-in.
Every change is checked against the live system before application.

---

## Persistence and rollback

- Runtime sysctls are persisted in `/etc/sysctl.d/99-kernyx.conf`
- Each apply creates a transaction under `/var/lib/kernyx/transactions/`
- Original values and file backups are recorded
- Rollback restores actual previous values, never generic defaults
- Idempotent: re-running the same policy reports no changes

---

## Supported systems

- Ubuntu 20.04, 22.04, 24.04, 26.04
- Debian 11, 12, 13
- Linux Mint, Pop!_OS
- Architecture: amd64 (ARM64 packages generated by release workflow)

Compatibility beyond these is not claimed.

---

## Architecture

```text
bin:      kernyx (Rust)
src/
  cli.rs        - Clap command definitions and aliases
  detect.rs     - /proc, /sys, command-based hardware detection
  hardware.rs   - strongly typed hardware model
  calc.rs       - hardware-derived calculations
  plan.rs       - plan items, reasons, risks
  apply.rs      - apply orchestration, verification
  tx.rs         - transactions, manifest, backup/rollback
  persist.rs    - persistence helpers (via apply.rs)
  report.rs     - reports
  backup.rs     - snapshots
  doctor.rs     - preflight diagnostics
  inspect.rs    - read-only inspectors
  monitor.rs    - btop adapter
  fetch.rs      - fastfetch adapter
  benchmark.rs  - hyperfine adapter
  config.rs     - configuration display/validation
vendor/btop/      - btop source and license
third_party/      - upstream licenses and credits
etc/btop.conf     - managed monitoring configuration
```

---

## Build from source

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build --release
sudo install -m 755 target/release/kernyx /usr/bin/kernyx
```

---

## CI and releases

- `.github/workflows/ci.yml` — fmt, clippy, tests, build
- `.github/workflows/release.yml` — tag-triggered release pipeline

Release workflow (on `v*` tags or manual dispatch):
1. validates version
2. runs tests
3. builds amd64 and arm64 binaries/packages
4. validates metadata and checksums
5. publishes GitHub Release artifacts
6. generates and signs APT metadata
7. deploys to `kernyx-apt` Pages repo atomically
8. verifies the published repository

Required GitHub Actions Secrets:
- `KERNYX_GPG_PRIVATE_KEY` — repository signing key
- `KERNYX_APT_TOKEN` — PAT with write access to `kernyx-apt`

Never commit private keys.

---

## Troubleshooting

| Problem | Solution |
|---|---|
| `apt update` signature error | Re-download `kernyx.asc` and verify fingerprint |
| `kernyx` not found after install | `hash -r` or reopen shell |
| `kernyx monitor` fails | Requires an interactive TTY |
| Unsupported distribution | Open an issue on GitHub |
| Rollback needed | `kernyx rollback --tx <ID>` |

---

## License and credits

- KERNYX: MIT — see [LICENSE](LICENSE)
- Upstream authors and licenses: [third_party/CREDITS.md](third_party/CREDITS.md)

No telemetry. No daemon by default. No unnecessary network access.
