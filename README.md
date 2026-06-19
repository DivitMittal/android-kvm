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
- A lan-mouse-backed edge capture runtime using `input-capture`/`input-event`.
- Relative pointer motion forwarding from the OS capture backend into Android.
- Virtual Android pointer bounds tracking for lan-mouse-style return to the host.

Mouse motion, mouse buttons, scroll, and common keyboard keys are forwarded through scrcpy's UHID control path.

Swipe through the configured host edge to enter Android. Merely resting at the edge is not enough: the pointer must keep moving outward by `activation-pixels` before Android focus starts. When Android focus is active, move back across the Android edge opposite the configured host edge to return to the host. For example, with `android-edge = "right"`, swipe through the host's right edge to enter Android, then move left to Android's left edge to release capture back to the host.

The capture layer uses GPL-3.0-or-later lan-mouse crates, so this project is licensed as GPL-3.0-or-later.

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

Override the configured phone placement for one run:

```bash
cargo run -- --android-edge left run
```

Default backend command:

```bash
scrcpy --no-video --no-window --audio-buffer=200 --keyboard=uhid --mouse=uhid --mouse-bind=bhsn --shortcut-mod=rctrl
```

At runtime, android-kvm owns the scrcpy control socket directly for UHID input. When `audio-enabled = true`, it also starts an audio-only scrcpy process with `--no-control` so audio routes to the host without competing for input control. By default, `audio-always-on = true` keeps this audio route active even while host focus is active. Set it to `false` to start audio only while Android focus is active.

## Configuration

By default, android-kvm reads:

```text
${XDG_CONFIG_HOME:-~/.config}/android-kvm/config.toml
```

Example:

```toml
android-edge = "right"
activation-pixels = 24
release-pixels = 4
poll-interval-ms = 16
pointer-scale = 1.0
audio-always-on = true
adb-binary = "adb"
android-width = 1080
android-height = 2400

[scrcpy]
binary = "scrcpy"
serial = "DEVICE_SERIAL"
no-video = true
no-window = true
audio-enabled = true
audio-buffer-ms = 200
keyboard = "uhid"
mouse = "uhid"
mouse-bind = "bhsn"
shortcut-mod = "rctrl"
extra-args = []
```

Set `android-edge` to where the Android device sits relative to the host display: `left`, `right`, `top`, or `bottom`. Use `--android-edge <edge>` to override it for a single CLI invocation. Set `audio-always-on = false` if you prefer Android audio only while actively focused. Set `activation-pixels` to the outward swipe distance required after hitting the host edge. Increase it if accidental edge activation is still too easy. Set `android-width` and `android-height` to your Android display size so edge-return tracking matches the device bounds. If omitted, android-kvm uses a 1080x2400 virtual display.

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
    activation-pixels = 24;
    release-pixels = 4;
    poll-interval-ms = 16;
    pointer-scale = 1.0;
    audio-always-on = true;
    adb-binary = "adb";
    scrcpy = {
      no-window = true;
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

## Platform support

android-kvm follows lan-mouse's host-input model: it asks `input-capture` to pick the first available backend for the current OS instead of hard-coding one backend in this project.

| Host OS | Runtime support | Packaging support |
| --- | --- | --- |
| Linux / Wayland | Uses lan-mouse `input-capture` backends such as layer-shell and input-capture portal when available. | Nix package/dev shell include the Linux X11 libraries required by lan-mouse's optional X11 backend. |
| Linux / X11 | Uses lan-mouse's X11 capture backend when available. | Nix package/dev shell include `libX11` and `libXtst`. |
| macOS | Uses lan-mouse's macOS capture backend; grant Accessibility/Input Monitoring permissions if edge capture does not start. | Exposed through the flake for `x86_64-darwin` and `aarch64-darwin`. |
| Windows | Uses lan-mouse's Windows capture backend when built with Cargo on Windows. | Nix does not package Windows targets; use the Rust/Cargo workflow on Windows. |

The Nix flake uses the same default Linux/Darwin system set as lan-mouse (`x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, and `aarch64-darwin`) and marks the package metadata as available on all platforms supported by the underlying Rust dependencies.

Every host still needs `adb`, `scrcpy`, USB access to the Android device, and any OS-specific input-capture permissions.
