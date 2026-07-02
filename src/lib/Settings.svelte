<script lang="ts">
  import {
    getProfile,
    saveProfile,
    checkEnvironment,
    cleanupDownloads,
    resolveDayzPath,
    installUpdate,
    type Profile,
    type EnvReport,
  } from "./api";
  import { updateState, refreshUpdateStatus } from "./update.svelte";
  import { showError, showMessage } from "./dialog";
  import { open } from "@tauri-apps/plugin-dialog";
  import { theme, setTheme, type ThemePref } from "./theme.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";
  import type { Component } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import XIcon from "@lucide/svelte/icons/x";
  import Palette from "@lucide/svelte/icons/palette";
  import User from "@lucide/svelte/icons/user";
  import HardDriveDownload from "@lucide/svelte/icons/hard-drive-download";
  import Activity from "@lucide/svelte/icons/activity";
  import ArrowUpCircle from "@lucide/svelte/icons/arrow-up-circle";
  import Sun from "@lucide/svelte/icons/sun";
  import Moon from "@lucide/svelte/icons/moon";
  import Monitor from "@lucide/svelte/icons/monitor";

  let profile = $state<Profile | null>(null);
  let saved = $state(false);
  let env = $state<EnvReport | null>(null);
  let cleaning = $state(false);
  let cleanupMsg = $state<string | null>(null);
  // App self-update: `st` is the shared status; the flags gate the two async actions.
  let st = $derived(updateState.status);
  let checking = $state(false);
  let updating = $state(false);
  let updateMsg = $state<string | null>(null);
  // Real host path behind a document-portal steam_root, shown so the user sees /mnt/... instead of
  // the opaque /run/user/.../doc/... mount.
  let steamDisplay = $state<string | null>(null);

  // A document-portal FUSE path (/run/user/<uid>/doc/... or /run/flatpak/doc/...); the picker hands
  // these back for folders outside the sandbox.
  function isPortalPath(p: string | null): boolean {
    return !!p && (p.startsWith("/run/flatpak/doc/") || /^\/run\/user\/[^/]+\/doc\//.test(p));
  }

  async function loadP() {
    profile = await getProfile();
  }
  async function loadEnv() {
    try {
      env = await checkEnvironment();
      // Surface the real host path when the configured folder is an opaque portal mount.
      if (isPortalPath(profile?.steam_root ?? null) && env.dayz_path) {
        steamDisplay = env.dayz_path;
      }
    } catch (e) {
      showError(e);
    }
  }
  async function saveP() {
    if (!profile) return;
    try {
      await saveProfile(profile);
    } catch (e) {
      showError(e);
      return;
    }
    saved = true;
    setTimeout(() => (saved = false), 1500);
    // Re-probe so the DayZ-install diagnostic reflects a changed override immediately.
    loadEnv();
  }
  async function browseDayz(seed?: string) {
    if (!profile) return;
    try {
      const dir = await open({
        directory: true,
        // Pre-aim the picker at the library Steam says owns DayZ (best-effort; the portal may
        // ignore it), else the currently-configured folder.
        defaultPath: seed ?? profile.steam_root ?? undefined,
        title: "Select your Steam library folder (contains steamapps)",
      });
      if (typeof dir === "string") {
        // In a Flatpak the picker returns a document-portal path (/run/user/.../doc/...) for
        // folders outside the sandbox; resolve/validate it before saving.
        const res = await resolveDayzPath(dir);
        if (!res.ok) {
          showMessage({
            title: "Select the Steam library folder",
            message:
              res.message ?? "That folder isn't a DayZ Steam library.",
          });
          return;
        }
        profile.steam_root = res.steam_root;
        steamDisplay = res.display_path;
        await saveP();
      }
    } catch (e) {
      showError(e);
    }
  }
  async function cleanup() {
    cleaning = true;
    cleanupMsg = null;
    try {
      const r = await cleanupDownloads();
      if (r.steam_running) {
        cleanupMsg =
          r.pending > 0
            ? `Steam is running — close it fully, then click again to remove ${r.pending} leftover download${r.pending === 1 ? "" : "s"}.`
            : "Steam is running — close it fully before cleaning up.";
      } else if (r.removed > 0) {
        cleanupMsg = `Removed ${r.removed} leftover download${r.removed === 1 ? "" : "s"}.`;
      } else {
        cleanupMsg = "No leftover downloads to clean up.";
      }
    } catch (e) {
      showError(e);
    } finally {
      cleaning = false;
    }
  }
  async function checkUpdates() {
    checking = true;
    updateMsg = null;
    try {
      const s = await refreshUpdateStatus();
      if (!s) return;
      if (s.latest_version == null) {
        updateMsg = "Couldn't reach GitHub to check. Try again later.";
      } else if (!s.update_available) {
        updateMsg = `Up to date (${s.current_version}).`;
      }
      // When an update *is* available the button block below renders instead of a message.
    } finally {
      checking = false;
    }
  }
  async function applyUpdate() {
    updating = true;
    updateMsg = null;
    try {
      // On success the process restarts into the new version and never returns here.
      await installUpdate();
    } catch (e) {
      showError(e);
    } finally {
      updating = false;
    }
  }
  loadP();
  loadEnv();
</script>

{#snippet sectionHead(Icon: Component, title: string)}
  <div class="flex items-center gap-2">
    <Icon class="text-primary size-4" />
    <h3 class="font-display text-xs font-semibold tracking-wider uppercase">{title}</h3>
  </div>
{/snippet}

{#snippet statusRow(label: string, ok: boolean, value: string)}
  <div class="flex items-baseline gap-3">
    <span class="text-muted-foreground w-28 shrink-0 text-sm">{label}</span>
    <span class="inline-flex min-w-0 items-baseline gap-1.5 text-sm">
      {#if ok}
        <Check class="text-online size-4 shrink-0 translate-y-0.5" />
      {:else}
        <XIcon class="text-destructive size-4 shrink-0 translate-y-0.5" />
      {/if}
      <span class="{ok ? '' : 'text-destructive'} break-all">{value}</span>
    </span>
  </div>
{/snippet}

{#if profile}
  <div class="mx-auto flex max-w-xl flex-col gap-4">
    <Card.Root>
      <Card.Content class="flex flex-col gap-3">
        {@render sectionHead(Palette, "Appearance")}
        <div class="flex flex-col gap-1.5">
          <Label class="text-muted-foreground text-xs">Theme</Label>
          <ToggleGroup.Root
            type="single"
            variant="outline"
            value={theme.pref}
            onValueChange={(v) => v && setTheme(v as ThemePref)}
          >
            <ToggleGroup.Item value="light"><Sun class="size-3.5" /> Light</ToggleGroup.Item>
            <ToggleGroup.Item value="dark"><Moon class="size-3.5" /> Dark</ToggleGroup.Item>
            <ToggleGroup.Item value="system"><Monitor class="size-3.5" /> System</ToggleGroup.Item>
          </ToggleGroup.Root>
        </div>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Content class="flex flex-col gap-4">
        {@render sectionHead(User, "Profile")}
        <div class="flex flex-col gap-1.5">
          <Label for="player" class="text-muted-foreground text-xs">Player name</Label>
          <Input id="player" bind:value={profile.player} placeholder="survivor" />
        </div>

        <div class="flex flex-col gap-1.5">
          <Label for="steam-root" class="text-muted-foreground text-xs">DayZ install location</Label>
          <div class="flex gap-2">
            <Input
              id="steam-root"
              class="flex-1 font-mono text-xs"
              bind:value={profile.steam_root}
              placeholder="auto-detect (e.g. /mnt/FAST/SteamLibrary)"
            />
            <Button variant="outline" type="button" onclick={() => browseDayz()}>Browse…</Button>
          </div>
          <span class="text-muted-foreground text-xs">
            Folder containing <code>steamapps</code>. Leave blank to detect automatically from
            your Steam libraries.
          </span>
          {#if isPortalPath(profile.steam_root) && steamDisplay}
            <span class="text-muted-foreground text-xs">
              Granted via the file portal → <code class="break-all">{steamDisplay}</code>
            </span>
          {/if}
        </div>

        <div class="flex items-center gap-2.5">
          <Button onclick={saveP}>Save</Button>
          {#if saved}<Badge variant="online">Saved ✓</Badge>{/if}
        </div>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Content class="flex flex-col gap-3">
        {@render sectionHead(HardDriveDownload, "Downloads")}
        <p class="text-muted-foreground text-sm">
          Mods download through the running Steam client — no SteamCMD, no separate login.
          Just keep Steam open and logged in when you Play.
        </p>
        <div class="flex flex-col gap-1.5">
          <div class="flex flex-wrap items-center gap-2.5">
            <Button variant="outline" type="button" onclick={cleanup} disabled={cleaning}>
              {cleaning ? "Cleaning…" : "Clean up leftover mod downloads"}
            </Button>
            {#if cleanupMsg}<span class="text-primary text-sm">{cleanupMsg}</span>{/if}
          </div>
          <span class="text-muted-foreground text-xs">
            Removes partial mod downloads left by a cancelled launch so Steam stops re-downloading
            them. Close Steam first — it must be shut down for cleanup to take effect.
          </span>
        </div>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Content class="flex flex-col gap-3">
        {@render sectionHead(ArrowUpCircle, "Updates")}
        <div class="flex items-baseline gap-3">
          <span class="text-muted-foreground w-28 shrink-0 text-sm">App version</span>
          <span class="font-mono text-sm break-all">{env?.app_version ?? "…"}</span>
          {#if st?.update_available}<span class="text-warn text-xs">● update available</span>{/if}
        </div>
        <div class="flex flex-col gap-2">
          <div class="flex flex-wrap items-center gap-2.5">
            <Button
              variant="outline"
              size="sm"
              type="button"
              onclick={checkUpdates}
              disabled={checking || updating}
            >
              {checking ? "Checking…" : "Check for updates"}
            </Button>
            {#if st?.update_available}
              <span class="text-sm">Update available: {st.current_version} → {st.latest_version}</span>
            {:else if updateMsg}
              <span class="text-muted-foreground text-sm">{updateMsg}</span>
            {/if}
          </div>
          {#if st?.update_available}
            {#if st.apply_supported}
              <div class="flex flex-wrap items-center gap-2.5">
                <Button variant="accent" size="sm" type="button" onclick={applyUpdate} disabled={updating}>
                  {updating ? (st.backend === "flatpak" ? "Updating…" : "Downloading…") : "Update & restart"}
                </Button>
                <span class="text-muted-foreground text-xs">
                  {st.backend === "flatpak"
                    ? "Installs the update via Flatpak and restarts dayzlin."
                    : "Downloads the new AppImage, verifies its signature, and restarts dayzlin."}
                </span>
              </div>
            {:else}
              <span class="text-muted-foreground text-xs">
                This build can't update itself — download the latest release from
                <code class="break-all">github.com/nikolaj95o/dayzlin/releases/latest</code>.
              </span>
            {/if}
          {/if}
        </div>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Content class="flex flex-col gap-3">
        {@render sectionHead(Activity, "Diagnostics")}
        {#if env}
          <div class="flex flex-col gap-2.5">
            {@render statusRow(
              "Steam running",
              env.steam_running,
              env.steam_running ? "running" : "not running — open Steam and log in to download mods",
            )}
            {@render statusRow(
              "Steam install",
              env.steam_found,
              env.steam_found ? `${env.steam_root} (${env.steam_kind})` : "not found",
            )}
            {@render statusRow(
              "DayZ installed",
              env.dayz_installed,
              env.dayz_installed
                ? (env.dayz_path ?? "installed")
                : "not found — set the DayZ install location above, or restart Steam with the drive mounted",
            )}
            {#if !env.dayz_installed && env.dayz_library_hint}
              <div class="flex flex-col gap-1.5">
                <Button
                  variant="outline"
                  size="sm"
                  type="button"
                  onclick={() => browseDayz(env!.dayz_library_hint!)}
                >
                  Grant access to {env.dayz_library_hint}
                </Button>
                <span class="text-muted-foreground text-xs">
                  Steam has DayZ here, but it's on a drive this app can't read until you grant
                  access. Click to pick that folder in the file chooser.
                </span>
              </div>
            {/if}
            {#if env.dayz_version}
              {@render statusRow("DayZ version", true, env.dayz_version)}
            {/if}
          </div>
        {:else}
          <p class="text-muted-foreground text-sm">Checking environment…</p>
        {/if}
      </Card.Content>
    </Card.Root>
  </div>
{/if}
