______________________________________________________________________

## description: Security rules for android-kvm applyTo: "\*\*"

# Security

## Command Execution

- Always pass binary paths and arguments as separate `OsString` elements via `Command::new()` — never interpolate into a shell string.
- `binary` and `adb-binary` config fields execute arbitrary programs by user intent; do not restrict legitimate custom paths.
- `extra_args` must remain a `Vec<String>` extended as discrete args, never joined into a shell command.

## Config File

- Config at `~/.config/android-kvm/config.toml` controls which binaries are executed — recommend `chmod 600` in documentation.
- Never log the full config struct if it ever gains secret fields (e.g. ADB auth tokens).

## UHID / Control Socket

- Only write valid scrcpy protocol messages to the control pipe — no user-controlled data should flow into raw bytes without protocol encoding.
- The control socket is local IPC only — must not be exposed over the network.

## Input Forwarding

- Input events forward from host to Android only while capture is active — capture state must be authoritative and not settable remotely.
- Keyboard events must not be forwarded to host applications while Android focus is active.
