# dayzlin

A graphical **DayZ launcher for Linux** — browse and filter community servers,
install the mods they require through the running Steam client, then launch and connect.
Built with Rust + Tauri and shipped as a self-contained Flatpak (AppImage fallback).

> **Status: early (0.1.x).** The domain logic and UI are in place and unit-tested,
> but the full play loop has **not yet been validated against a real Steam/DayZ
> install** and no packaged build has been verified end-to-end. Treat this as a
> work in progress, not a stable release. See [Roadmap to 1.0](#roadmap-to-10).

## Why dayzlin?

I built dayzlin because I couldn't find a simple, easy-to-install way to play modded
DayZ on Linux. The alternatives each got in the way:

- Other community launchers are complicated to set up, and some ship with poor
  performance, major bugs, or unintuitive UIs that sit between you and the game.
- The **official launcher subscribes** you to each server's mods in the Steam Workshop —
  and then DayZ **won't start until every subscribed mod has finished downloading**, so a
  single large or slow mod blocks the whole game.

dayzlin aims to be the opposite: install it in one command (Flatpak), point it at a
server, and play. It fetches the mods a server needs as **one-shot** Steam downloads
without subscribing you to them — so nothing clutters your Workshop subscriptions and no
forced pre-download blocks the game from launching.

## Features

- Server browser: live server list with filters (map, first-person, password,
  free slots, mod count, players) and fuzzy search.
- Mod handling: detects installed mods, computes what a server still needs, and
  asks the running Steam client to download them; symlinks mods into the game directory.
- One-click play: builds the DayZ launch arguments and connects to the server.
- Settings: player name and DayZ location; favorites and history (WIP).
- Works with both **native** Steam (`~/.steam/steam`) and **Flatpak** Steam
  (`~/.var/app/com.valvesoftware.Steam`).

## Requirements

- Linux (x86-64). No Windows/macOS support.
- **Steam** installed, with **DayZ** (App ID 221100) owned and installed.
- A GPU/driver capable of running DayZ under Proton.

## Steam setup

There is no separate login or extra dependency. dayzlin downloads mods by asking the
**Steam client you're already logged into** to fetch them (via a `steam://` workshop
download), then symlinks them into the game directory. Just make sure, before you Play:

- **Steam is running and logged in** (the Settings → Diagnostics panel shows "Steam
  running"). Mods can only download while it's open.
- Your account **owns DayZ** — required to download DayZ workshop items.

Downloads are one-shot (dayzlin does not subscribe you to the mods), so they don't clutter
your Steam Workshop subscriptions.

## Troubleshooting

- **Can't connect / "missing mod" on a mod-free server / stuck on the last server.** DayZ
  needs Steam to be **online** to authenticate against a server. If the Steam friends panel
  shows "disconnected" (offline mode), connections fail and the menu can keep showing the
  previous session's server and mods — which looks like a missing-mod error. Switch Steam
  back online (or restart it) and retry.
- **A mod won't download / the progress bar never moves.** Downloads are driven by the
  Steam client, so it must be **running and logged in** — check Settings → Diagnostics.
  If a download stalls, look in Steam for a paused/failed workshop download or free up disk
  space, then Play again.
- dayzlin launches via `steam -applaunch 221100 … -connect=<ip> -port=<port>`. It does **not**
  use the `steam://connect/<ip>:<port>` URI, which cannot load mods and would break modded
  servers.

## Install

### Flatpak (recommended — gets updates via `flatpak update`)

Add the dayzlin remote once, then install. Flathub is needed for the GNOME
runtime dayzlin builds against:

```bash
# one-time: add Flathub (runtime) and the dayzlin remote
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --if-not-exists dayzlin https://nikolaj95o.github.io/dayzlin/dayzlin.flatpakrepo

# install
flatpak install dayzlin io.github.nikolaj95o.dayzlin

# get the latest version any time
flatpak update
```

Each tagged release publishes a new commit to the remote, so `flatpak update`
pulls the newest build.

### Other options

- **AppImage / binary**: attached to each [GitHub Release](https://github.com/nikolaj95o/dayzlin/releases)
  (produced once a `v*` tag is pushed). One-shot — no auto-update.
- **Local Flatpak build**: see the build instructions in
  [`flatpak/io.github.nikolaj95o.dayzlin.yml`](flatpak/io.github.nikolaj95o.dayzlin.yml).

## Uninstall

### Flatpak

```bash
# remove the app together with its data (favorites, history, cached server list)
flatpak uninstall --delete-data io.github.nikolaj95o.dayzlin

# optional: also drop the dayzlin update remote
flatpak remote-delete dayzlin
```

### AppImage / binary

Delete the AppImage/binary you downloaded, then remove its data directory
(profile, favorites/history, cached server list):

```bash
rm -rf ~/.local/share/io.github.nikolaj95o.dayzlin
```

### Mods dayzlin linked (both install types)

dayzlin symlinks each server's mods into your DayZ folder
(`steamapps/common/DayZ/@<Mod>`) and downloads the underlying Workshop items through
Steam (`steamapps/workshop/content/221100/`). Uninstalling dayzlin leaves both in
place — they belong to Steam/DayZ. To reclaim the space, delete the `@<Mod>` symlinks
from the DayZ directory and remove the unwanted mods under `steamapps/workshop/content/221100/`.

## Build from source

Prerequisites: a Rust toolchain (stable), Node.js (≥ 24), and the Tauri Linux
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
  filter, mod state + Steam-client download orchestration, launch-arg building. No UI, no Tauri.
- **`src-tauri`** — thin Tauri v2 app exposing core functions as commands and
  streaming progress events. No business logic.
- **`src/`** — Svelte + TypeScript frontend (server browser, settings, play).

When running inside Flatpak, Steam calls are routed through
`flatpak-spawn --host` automatically.

## Roadmap to 1.0

1. Validate the complete loop on real hardware: browse → download missing mods via
   the Steam client → launch DayZ → connect (i.e. actually play through it).
2. Build and verify a Flatpak/AppImage on a clean system.
3. Wire up the remaining UI: record history on launch, add/remove favorites,
   surface error states (Steam not running, download stalled, Steam not found).

## Acknowledgements

dayzlin builds on the knowledge and approach of existing Linux DayZ tooling:

- [dayz-ctl](https://github.com/WoozyMasta/dayz-ctl) by WoozyMasta — a DayZ
  launcher whose workflow (mod handling, mod symlinking, Proton launch
  invocation) informed dayzlin's design.
- [dayzsalauncher](https://dayzsalauncher.com/) — dayzlin uses its public
  server-list API as the source of community server data.

dayzlin is built with heavy use of **AI coding assistants** — much of the code,
tests, and documentation is AI-generated, then reviewed and directed by a human.

## License

[GPL-3.0-or-later](LICENSE). You may use, study, modify, and share it; any
distributed derivative must remain open under the GPL.
