# Contributing to KERNYX

KERNYX is a Linux infrastructure tool. Changes must preserve safety and
reversibility.

## Development

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo build --release
```

## Rules

- Do not modify SSH access, firewall rules, boot configuration, or XDP/eBPF programs.
- Do not add hardcoded hardware assumptions.
- Every optimization must be calculated from detected hardware.
- Every applied change must be transactional and reversible.
- Do not fabricate benchmark or performance numbers.
- Keep the CLI non-interactive friendly and scriptable.

## Pull requests

- Keep changes focused.
- Add tests for new calculations or detection logic.
- Update documentation when behavior changes.
- Ensure CI passes before requesting review.
