# cmux-linux

Linux fork of [cmux](https://github.com/manaflow-ai/cmux) -- terminal multiplexer for AI coding agents.

Uses **VTE** (GTK4 terminal widget) instead of Ghostty for terminal rendering, making it work natively on Linux without waiting for Ghostty's embedded runtime API.

## Changes from upstream

- Replaced Ghostty rendering backend with VTE4
- Removed ghostty-sys and ghostty-gtk crates
- No Zig dependency -- pure Rust + GTK4/libadwaita
- Terminal titles auto-update from shell (shows current directory/command)

## Architecture

- `cmux/` -- Main application (GTK4/libadwaita + VTE4)
  - `model/` -- TabManager, Workspace, Panel, LayoutNode
  - `ui/` -- Window, Sidebar, SplitView, TerminalPanel
  - `socket/` -- Unix socket server, v2 JSON protocol, auth
  - `session/` -- Session persistence (XDG, JSON compatible with macOS cmux)
  - `notifications.rs` -- Notification store + desktop notifications
- `cmux-cli/` -- CLI client (`cmux workspace list`, `cmux surface send-text`, etc.)

## Building

```bash
# With nix
nix build

# With cargo (needs GTK4, libadwaita, VTE development libraries)
cargo build --release
```

## Socket Protocol

Unix socket at `$XDG_RUNTIME_DIR/cmux.sock` (falls back to `/tmp/cmux-$UID.sock`).
Line-delimited JSON v2 protocol. Compatible with macOS cmux socket API.

## License

AGPL-3.0-or-later -- same as upstream cmux.

Based on the [Linux port PR #828](https://github.com/manaflow-ai/cmux/pull/828) by shuhei0866.
