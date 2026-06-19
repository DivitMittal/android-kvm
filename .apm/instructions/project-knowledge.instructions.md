---
description: Project knowledge for android-kvm
applyTo: "**"
---

# Project Knowledge

## What This Repo Is

android-kvm is a Rust-first USB Android software KVM. It aims to provide lan-mouse-style edge switching while using scrcpy as the Android USB backend for keyboard, mouse, and optional audio transfer.

## Common Commands

```bash
nix develop
nix fmt
nix flake check
cargo test
cargo run -- run --dry-run
```

Run `cargo test` after Rust changes and `nix flake check` after Nix changes.

## Architecture

`flake.nix` uses flake-parts and import-tree. Flake modules live under `flake/`, and Home Manager modules live under `modules/home/`.

The Rust code is split by responsibility:

- `src/config.rs` loads TOML config from XDG config.
- `src/edge.rs` contains the shared edge enum used by CLI/config/runtime code.
- `src/scrcpy.rs` owns scrcpy process configuration checks; `src/scrcpy_control.rs` launches the direct `app_process` control server.

## Backend Direction

The backend integrates directly with scrcpy's control path so global host input capture can be forwarded without relying on a focused scrcpy window. Audio, when enabled, runs as a separate `scrcpy --no-control` process.
