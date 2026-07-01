<script lang="ts">
  import {
    getProfile,
    saveProfile,
    checkEnvironment,
    cleanupDownloads,
    resolveDayzPath,
    type Profile,
    type EnvReport,
  } from "./api";
  import { showError } from "./dialog";
  import { open } from "@tauri-apps/plugin-dialog";
  import { theme, setTheme, type ThemePref } from "./theme.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";

  let profile = $state<Profile | null>(null);
  let saved = $state(false);
  let env = $state<EnvReport | null>(null);
  let cleaning = $state(false);
  let cleanupMsg = $state<string | null>(null);

  async function loadP() {
    profile = await getProfile();
  }
  async function loadEnv() {
    try {
      env = await checkEnvironment();
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
  async function browseDayz() {
    if (!profile) return;
    try {
      const dir = await open({
        directory: true,
        title: "Select your Steam library folder (contains steamapps)",
      });
      if (typeof dir === "string") {
        // In a Flatpak the picker returns a document-portal path (/run/user/.../doc/...) for
        // folders outside the sandbox; resolve it back to the real host library root before saving.
        profile.steam_root = await resolveDayzPath(dir);
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
  loadP();
  loadEnv();
</script>

{#if profile}
  <Card.Root class="mt-3 max-w-md">
    <Card.Content class="flex flex-col gap-4">
      <div class="flex flex-col gap-1.5">
        <Label>Theme</Label>
        <ToggleGroup.Root
          type="single"
          variant="outline"
          value={theme.pref}
          onValueChange={(v) => v && setTheme(v as ThemePref)}
        >
          <ToggleGroup.Item value="light">Light</ToggleGroup.Item>
          <ToggleGroup.Item value="dark">Dark</ToggleGroup.Item>
          <ToggleGroup.Item value="system">System</ToggleGroup.Item>
        </ToggleGroup.Root>
      </div>

      <Separator />

      <div class="flex flex-col gap-1.5">
        <Label for="player">Player name</Label>
        <Input id="player" bind:value={profile.player} placeholder="survivor" />
      </div>

      <div class="flex flex-col gap-1.5">
        <Label for="steam-root">DayZ install location</Label>
        <div class="flex gap-2">
          <Input
            id="steam-root"
            class="flex-1"
            bind:value={profile.steam_root}
            placeholder="auto-detect (e.g. /mnt/FAST/SteamLibrary)"
          />
          <Button variant="outline" type="button" onclick={browseDayz}>Browse…</Button>
        </div>
        <span class="text-muted-foreground text-xs">
          Folder containing <code>steamapps</code>. Leave blank to detect automatically from
          your Steam libraries.
        </span>
      </div>

      <div class="flex items-center gap-2.5">
        <Button onclick={saveP}>Save</Button>
        {#if saved}<Badge variant="secondary">Saved ✓</Badge>{/if}
      </div>

      <Separator />

      <p class="text-muted-foreground text-sm">
        Mods download through the running Steam client — no SteamCMD, no separate login.
        Just keep Steam open and logged in when you Play.
      </p>
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center gap-2.5">
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

      <Separator />

      <h3 class="text-sm font-medium">Diagnostics</h3>
      {#if env}
        <div class="flex flex-col gap-2 text-sm">
          <div class="flex items-baseline gap-2.5">
            <span class="text-muted-foreground w-32 shrink-0">App version</span>
            <span class="break-all">{env.app_version}</span>
          </div>
          <div class="flex items-baseline gap-2.5">
            <span class="text-muted-foreground w-32 shrink-0">Steam running</span>
            {#if env.steam_running}
              <span class="break-all text-green-600 dark:text-green-400">✓ running</span>
            {:else}
              <span class="break-all text-red-600 dark:text-red-400">✗ not running — open Steam and log in to download mods</span>
            {/if}
          </div>
          <div class="flex items-baseline gap-2.5">
            <span class="text-muted-foreground w-32 shrink-0">Steam install</span>
            {#if env.steam_found}
              <span class="break-all text-green-600 dark:text-green-400">✓ {env.steam_root} ({env.steam_kind})</span>
            {:else}
              <span class="break-all text-red-600 dark:text-red-400">✗ not found</span>
            {/if}
          </div>
          <div class="flex items-baseline gap-2.5">
            <span class="text-muted-foreground w-32 shrink-0">DayZ installed</span>
            {#if env.dayz_installed}
              <span class="break-all text-green-600 dark:text-green-400">✓ {env.dayz_path}</span>
            {:else}
              <span class="break-all text-red-600 dark:text-red-400">
                ✗ not found — set the DayZ install location above, or restart Steam with the
                drive mounted
              </span>
            {/if}
          </div>
          {#if env.dayz_version}
            <div class="flex items-baseline gap-2.5">
              <span class="text-muted-foreground w-32 shrink-0">DayZ version</span>
              <span class="break-all text-green-600 dark:text-green-400">{env.dayz_version}</span>
            </div>
          {/if}
        </div>
      {:else}
        <p class="text-muted-foreground text-sm">Checking environment…</p>
      {/if}
    </Card.Content>
  </Card.Root>
{/if}
