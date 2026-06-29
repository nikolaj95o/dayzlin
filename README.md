# dayzlin

A graphical **DayZ launcher for Linux** — browse and filter community servers,
install/update the mods they require via SteamCMD, then launch and connect.
Built with Rust + Tauri and shipped as a self-contained Flatpak (AppImage fallback).

> **Status: early (0.1.x).** The domain logic and UI are in place and unit-tested,
> but the full play loop has **not yet been validated against a real Steam/DayZ
> install** and no packaged build has been verified end-to-end. Treat this as a
> work in progress, not a stable release. See [Roadmap to 1.0](#roadmap-to-10).

## Features

- Server browser: live server list with filters (map, first-person, password,
  free slots, mod count, players) and fuzzy search.
- Mod handling: detects installed mods, computes what a server still needs, and
  drives SteamCMD to download/update; symlinks mods into the game directory.
- One-click play: builds the DayZ launch arguments and connects to the server.
- Settings: player name and Steam login; favorites and history (WIP).
- Works with both **native** Steam (`~/.steam/steam`) and **Flatpak** Steam
  (`~/.var/app/com.valvesoftware.Steam`).

## Requirements

- Linux (x86-64). No Windows/macOS support.
- **Steam** installed, with **DayZ** (App ID 221100) owned and installed.
- **SteamCMD** available on the host (most distros package it) for mod installs.
- A GPU/driver capable of running DayZ under Proton.

## Steam setup (first time)

Installing mods uses **SteamCMD**, which keeps its **own** login, separate from the
Steam desktop client. Before your first mod install:

1. Enter your Steam account name in **Settings → Steam login** and Save.
2. Click **Set up Steam login** (or run `steamcmd +login <user> +quit` in a terminal).
   Enter your password and Steam Guard code once — SteamCMD caches the session, so later
   installs run non-interactively.
3. **Close the Steam client** while dayzlin downloads mods. SteamCMD and the client share
   the same Steam directory and will conflict otherwise.

Notes:
- The account must **own DayZ** — anonymous SteamCMD logins cannot download DayZ mods.
- If a download later fails with a login error, repeat the one-time login above (Steam
  Guard sessions can expire).

## Install

> Packaged artifacts are not yet published. For now, build from source (below).

- **Flatpak** (local build): see the build instructions in
  [`flatpak/io.github.nikolaj95o.dayzlin.yml`](flatpak/io.github.nikolaj95o.dayzlin.yml).
- **AppImage / binary**: produced by the release workflow once a `v*` tag is pushed.

## Build from source

Prerequisites: a Rust toolchain (stable), Node.js (≥ 20.19), and the Tauri Linux
system dependencies (`webkit2gtk-4.1`, `libappindicator`, `librsvg`, etc.).

```bash
# frontend deps
npm ci

# run in development (opens the app, hot-reloads the UI)
npm run tauri dev

# production build (AppImage + deb + rpm + binary under target/release/)
npm run tauri build
```

> **Arch / CachyOS note:** the AppImage step can fail with
> `strip: ... unknown type [0x13] section '.relr.dyn'` because the `strip`
> bundled in `linuxdeploy` is older than the system toolchain. Build with
> stripping disabled: `NO_STRIP=true npm run tauri build`.

The Rust core library lives in `crates/dayz-core` (UI-agnostic, fully unit-tested
behind a mockable command runner). The Tauri app crate is `src-tauri`; the Svelte
frontend is under `src/`.

```bash
cargo test --workspace      # core unit tests (no real Steam/network needed)
cargo clippy --workspace -- -D warnings
npm run check               # svelte-check / TypeScript
```

## Architecture

- **`crates/dayz-core`** — all domain logic: Steam detection, server fetch/cache/
  filter, mod state + SteamCMD orchestration, launch-arg building. No UI, no Tauri.
- **`src-tauri`** — thin Tauri v2 app exposing core functions as commands and
  streaming progress events. No business logic.
- **`src/`** — Svelte + TypeScript frontend (server browser, settings, play).

When running inside Flatpak, Steam/SteamCMD calls are routed through
`flatpak-spawn --host` automatically.

## Roadmap to 1.0

1. Validate the complete loop on real hardware: browse → install missing mods via
   SteamCMD → launch DayZ → connect (i.e. actually play through it).
2. Build and verify a Flatpak/AppImage on a clean system.
3. Wire up the remaining UI: record history on launch, add/remove favorites,
   surface error states (login expiry, anonymous account, Steam not found).

## Acknowledgements

dayzlin builds on the knowledge and approach of existing Linux DayZ tooling:

- [dayz-ctl](https://github.com/WoozyMasta/dayz-ctl) by WoozyMasta — a DayZ
  launcher whose workflow (SteamCMD mod handling, mod symlinking, Proton launch
  invocation) informed dayzlin's design.
- [dayzsalauncher](https://dayzsalauncher.com/) — dayzlin uses its public
  server-list API as the source of community server data.

## License

[GPL-3.0-or-later](LICENSE). You may use, study, modify, and share it; any
distributed derivative must remain open under the GPL.
