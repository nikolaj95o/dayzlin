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
  import { theme, setTheme } from "./theme.svelte";

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
  <div class="mt-3 flex max-w-[360px] flex-col gap-3.5">
    <div class="flex flex-col gap-1 text-sm text-text-h">
      Theme
      <div class="flex gap-1.5">
        <button class="btn" class:btn-active={theme.pref === "light"} onclick={() => setTheme("light")}>Light</button>
        <button class="btn" class:btn-active={theme.pref === "dark"} onclick={() => setTheme("dark")}>Dark</button>
        <button class="btn" class:btn-active={theme.pref === "system"} onclick={() => setTheme("system")}>System</button>
      </div>
    </div>
    <hr class="my-1 w-full border-0 border-t border-border" />
    <label class="flex flex-col gap-1 text-sm text-text-h">
      Player name
      <input class="field" bind:value={profile.player} placeholder="survivor" />
    </label>
    <label class="flex flex-col gap-1 text-sm text-text-h">
      DayZ install location
      <div class="flex gap-2">
        <input
          class="field flex-1"
          bind:value={profile.steam_root}
          placeholder="auto-detect (e.g. /mnt/FAST/SteamLibrary)"
        />
        <button class="btn" type="button" onclick={browseDayz}>Browse…</button>
      </div>
      <span class="text-xs leading-[1.4] text-text">
        Folder containing <code>steamapps</code>. Leave blank to detect automatically from
        your Steam libraries.
      </span>
    </label>
    <div class="flex items-center gap-2.5">
      <button class="btn" onclick={saveP}>Save</button>
      {#if saved}<span class="text-[13px] text-accent">Saved ✓</span>{/if}
    </div>
    <hr class="my-1 w-full border-0 border-t border-border" />
    <p class="text-[13px] leading-[1.45] text-text">
      Mods download through the running Steam client — no SteamCMD, no separate login.
      Just keep Steam open and logged in when you Play.
    </p>
    <div class="flex flex-col gap-1.5">
      <div class="flex items-center gap-2.5">
        <button class="btn" type="button" onclick={cleanup} disabled={cleaning}>
          {cleaning ? "Cleaning…" : "Clean up leftover mod downloads"}
        </button>
        {#if cleanupMsg}<span class="text-[13px] text-accent">{cleanupMsg}</span>{/if}
      </div>
      <span class="text-xs leading-[1.4] text-text">
        Removes partial mod downloads left by a cancelled launch so Steam stops re-downloading
        them. Close Steam first — it must be shut down for cleanup to take effect.
      </span>
    </div>
    <hr class="my-1 w-full border-0 border-t border-border" />
    <h3 class="m-0 text-sm text-text-h">Diagnostics</h3>
    {#if env}
      <div class="flex flex-col gap-2">
        <div class="flex items-baseline gap-2.5 text-[13px]">
          <span class="shrink-0 grow-0 basis-[130px] text-text">App version</span>
          <span class="break-all text-text-h">{env.app_version}</span>
        </div>
        <div class="flex items-baseline gap-2.5 text-[13px]">
          <span class="shrink-0 grow-0 basis-[130px] text-text">Steam running</span>
          {#if env.steam_running}
            <span class="break-all text-green-600 dark:text-green-400">✓ running</span>
          {:else}
            <span class="break-all text-red-600 dark:text-red-400">✗ not running — open Steam and log in to download mods</span>
          {/if}
        </div>
        <div class="flex items-baseline gap-2.5 text-[13px]">
          <span class="shrink-0 grow-0 basis-[130px] text-text">Steam install</span>
          {#if env.steam_found}
            <span class="break-all text-green-600 dark:text-green-400">✓ {env.steam_root} ({env.steam_kind})</span>
          {:else}
            <span class="break-all text-red-600 dark:text-red-400">✗ not found</span>
          {/if}
        </div>
        <div class="flex items-baseline gap-2.5 text-[13px]">
          <span class="shrink-0 grow-0 basis-[130px] text-text">DayZ installed</span>
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
          <div class="flex items-baseline gap-2.5 text-[13px]">
            <span class="shrink-0 grow-0 basis-[130px] text-text">DayZ version</span>
            <span class="break-all text-green-600 dark:text-green-400">{env.dayz_version}</span>
          </div>
        {/if}
      </div>
    {:else}
      <p class="text-[13px] leading-[1.45] text-text">Checking environment…</p>
    {/if}
  </div>
{/if}
