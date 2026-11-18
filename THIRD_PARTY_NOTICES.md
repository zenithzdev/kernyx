# Third-party notices

KERNYX integrates the components below. Each is used under its own license and
each keeps its original copyright notices. Nothing in this project's license
grants rights over their code.

## fastfetch — MIT License

Source: `third_party/fastfetch/LICENSE`. The compiled fastfetch binary ships as
`/usr/lib/kernyx/kernyx-fetch`; this notice must ship with it.

    Copyright (c) 2021-2023 Linus Dierheimer
    Copyright (c) 2022-2026 Carter Li

## btop++ — Apache License 2.0

Source: `vendor/btop/LICENSE`, shipped verbatim as
`/usr/share/doc/kernyx/btop-LICENSE`. The compiled btop binary ships as
`/usr/lib/kernyx/kernyx-monitor`.

    Copyright (c) 2021-2024 Jakob P. Liljenberg a.k.a. "Aristocratos"

btop++ bundles additional components with their own notices, retained in
place:

- **{fmt}** — MIT License — Copyright (c) 2012 - present, Victor Zverovich
  and {fmt} contributors — `vendor/btop/include/fmt/`
- **Intel GPU top / DRM headers** — Copyright (c) Dave Airlie, Jakob
  Bornecrantz, Intel Corporation, Red Hat Inc., Tungsten Graphics Inc. —
  `vendor/btop/src/linux/intel_gpu_top/`
- **Platform-specific collectors** — Copyright (c) Brian Callahan (OpenBSD),
  NetBSD and FreeBSD contributors — `vendor/btop/src/openbsd/`,
  `vendor/btop/src/netbsd/`, `vendor/btop/src/freebsd/`
- **Themes** — individual theme authors, credited in each file under
  `vendor/btop/themes/` (e.g. BachoSeven, Pietryszak, Marco Radocchia,
  Daniel Ruiz de Alegría, Kyli0x, kk9uk and others)

## hyperfine — MIT OR Apache-2.0

Source: `third_party/hyperfine-LICENSE-MIT`,
`third_party/hyperfine-LICENSE-APACHE`. The compiled hyperfine binary ships as
`/usr/lib/kernyx/kernyx-bench`.

    Copyright (c) 2018-2024 David Peter (sharkdp)

## Project license

KERNYX itself is MIT licensed — see LICENSE. Copyright (c) 2026 Zenithz Dev.
