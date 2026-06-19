# android-kvm

Rust-first USB Android software KVM backed by scrcpy.

The goal is lan-mouse-style edge switching for Android over USB: move the host pointer past a configured screen edge, capture host input, and forward keyboard/mouse events to Android through scrcpy's UHID path.

## Status

This is an initial buildable scaffold. It currently provides:

- A Rust CLI.
- Nix dev shell and package definition.
- TOML configuration loading.
- A scrcpy backend launcher matching the known-good UHID/no-video command.
- Optional scrcpy audio transfer configuration.
- A Home Manager module exported as `homeManagerModules.android-kvm` and `homeManagerModules.default`.
- A tested edge-transition state machine ready to connect to OS input capture.

Real global input capture is the next milestone.

## Usage

Enter the dev shell:

```bash
nix develop
```

Check that scrcpy is available:

```bash
cargo run -- check
```

Print the scrcpy command that would run:

```bash
cargo run -- run --dry-run
```

Run scrcpy:

```bash
cargo run -- run
```

Default backend command:

```bash
scrcpy --no-video --audio-buffer=200 --keyboard=uhid --mouse=uhid --mouse-bind=bhsn --shortcut-mod=rctrl
```

## Configuration

By default, android-kvm reads:

```text
${XDG_CONFIG_HOME:-~/.config}/android-kvm/config.toml
```

Example:

```toml
android-edge = "right"
activation-pixels = 1
release-pixels = 4

[scrcpy]
binary = "scrcpy"
serial = "DEVICE_SERIAL"
no-video = true
audio-enabled = true
audio-buffer-ms = 200
keyboard = "uhid"
mouse = "uhid"
mouse-bind = "bhsn"
shortcut-mod = "rctrl"
extra-args = []
```

## Home Manager

Import the module from the flake and configure `programs.android-kvm`.

```nix
imports = [
  inputs.android-kvm.homeManagerModules.android-kvm
];
```

Example settings:

```nix
programs.android-kvm = {
  enable = true;
  package = inputs.android-kvm.packages.${pkgs.stdenv.hostPlatform.system}.default;
  settings = {
    android-edge = "right";
    activation-pixels = 1;
    release-pixels = 4;
    scrcpy = {
      audio-enabled = true;
      audio-buffer-ms = 200;
      keyboard = "uhid";
      mouse = "uhid";
      mouse-bind = "bhsn";
      shortcut-mod = "rctrl";
    };
  };
};
```

## Roadmap

1. Linux/X11 input watcher and pointer clamp.
2. Linux evdev/uinput backend for global mouse and keyboard capture.
3. Direct scrcpy control-channel integration instead of process-only launch.
4. Wayland support through compositor-supported protocols where available.
5. macOS and Windows capture backends.
