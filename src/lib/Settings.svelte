<script lang="ts">
  import {
    getProfile,
    saveProfile,
    setupSteamLogin,
    checkEnvironment,
    type Profile,
    type EnvReport,
  } from "./api";
  import { showError, showMessage } from "./dialog";
  import { open } from "@tauri-apps/plugin-dialog";
  import { theme, setTheme } from "./theme.svelte";

  let profile = $state<Profile | null>(null);
  let saved = $state(false);
  let env = $state<EnvReport | null>(null);

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
        profile.steam_root = dir;
        await saveP();
      }
    } catch (e) {
      showError(e);
    }
  }
  async function setupLogin() {
    if (!profile) return;
    // Persist the current login first — the helper reads it from disk.
    await saveProfile(profile);
    try {
      await setupSteamLogin();
      showMessage({
        title: "Steam login started",
        message:
          "A terminal opened running steamcmd. Enter your password and Steam Guard " +
          "code there, wait for 'Waiting for user info...OK', then close it and try " +
          "Play again. Close the Steam client during downloads.",
      });
    } catch (e) {
      showError(e);
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
      Steam login
      <input class="field" bind:value={profile.steam_login} placeholder="steam username" />
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
      First-time mod installs need a one-time Steam login (steamcmd keeps its own
      credentials, separate from the Steam client). This opens a terminal — complete
      Steam Guard there once.
    </p>
    <div class="flex items-center gap-2.5">
      <button class="btn" onclick={setupLogin}>Set up Steam login</button>
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
          <span class="shrink-0 grow-0 basis-[130px] text-text">SteamCMD</span>
          {#if env.steamcmd_installed}
            <span class="break-all text-green-600 dark:text-green-400">✓ installed</span>
          {:else}
            <span class="break-all text-red-600 dark:text-red-400">✗ not found — install the steamcmd package</span>
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
        <div class="flex items-baseline gap-2.5 text-[13px]">
          <span class="shrink-0 grow-0 basis-[130px] text-text">Terminal for login</span>
          {#if env.terminal}
            <span class="break-all text-green-600 dark:text-green-400">✓ {env.terminal}</span>
          {:else}
            <span class="break-all text-red-600 dark:text-red-400">✗ none found</span>
          {/if}
        </div>
        <div class="flex items-baseline gap-2.5 text-[13px]">
          <span class="shrink-0 grow-0 basis-[130px] text-text">Steam account</span>
          {#if env.steam_login && env.steam_login !== "anonymous"}
            <span class="break-all text-green-600 dark:text-green-400">✓ {env.steam_login}</span>
          {:else}
            <span class="break-all text-red-600 dark:text-red-400">✗ not set</span>
          {/if}
        </div>
      </div>
      <p class="text-[13px] leading-[1.45] text-text">Actual Steam login is verified when you Play.</p>
    {:else}
      <p class="text-[13px] leading-[1.45] text-text">Checking environment…</p>
    {/if}
  </div>
{/if}
