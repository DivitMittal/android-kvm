# Contributing

Contributions are welcome: bug reports, platform backends, packaging fixes, and improvements to the scrcpy integration.

## Setup

```sh
nix develop
```

## Guidelines

- Rust files: format with `rustfmt`.
- Nix files: format with `nix fmt`.
- Keep platform-specific input capture isolated behind backend boundaries.
- Run `cargo test` and `nix flake check` before submitting.
- Test on the target host platform when changing input capture or scrcpy behavior.

## Submitting Changes

1. Fork the repo and create a branch: `feat/description` or `fix/description`.
2. Keep commits atomic and use Conventional Commits format.
3. Open a PR against `main` with a clear description of what changed and why.

## Reporting Issues

Open a GitHub issue with:

- Host OS, desktop environment/compositor, and Android version.
- `scrcpy --version` output.
- The android-kvm config used.
- Steps to reproduce.
- Expected vs actual behavior.
