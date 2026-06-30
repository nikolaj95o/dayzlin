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
  <div class="settings">
    <label>
      Player name
      <input bind:value={profile.player} placeholder="survivor" />
    </label>
    <label>
      Steam login
      <input bind:value={profile.steam_login} placeholder="steam username" />
    </label>
    <label>
      DayZ install location
      <div class="field-row">
        <input
          bind:value={profile.steam_root}
          placeholder="auto-detect (e.g. /mnt/FAST/SteamLibrary)"
        />
        <button type="button" onclick={browseDayz}>Browse…</button>
      </div>
      <span class="field-hint">
        Folder containing <code>steamapps</code>. Leave blank to detect automatically from
        your Steam libraries.
      </span>
    </label>
    <div class="actions">
      <button onclick={saveP}>Save</button>
      {#if saved}<span class="saved">Saved ✓</span>{/if}
    </div>
    <hr />
    <p class="hint">
      First-time mod installs need a one-time Steam login (steamcmd keeps its own
      credentials, separate from the Steam client). This opens a terminal — complete
      Steam Guard there once.
    </p>
    <div class="actions">
      <button onclick={setupLogin}>Set up Steam login</button>
    </div>
    <hr />
    <h3 class="diag-title">Diagnostics</h3>
    {#if env}
      <div class="diag">
        <div class="diag-row">
          <span class="diag-label">App version</span>
          <span class="diag-value">{env.app_version}</span>
        </div>
        <div class="diag-row">
          <span class="diag-label">SteamCMD</span>
          {#if env.steamcmd_installed}
            <span class="diag-value ok">✓ installed</span>
          {:else}
            <span class="diag-value bad">✗ not found — install the steamcmd package</span>
          {/if}
        </div>
        <div class="diag-row">
          <span class="diag-label">Steam install</span>
          {#if env.steam_found}
            <span class="diag-value ok">✓ {env.steam_root} ({env.steam_kind})</span>
          {:else}
            <span class="diag-value bad">✗ not found</span>
          {/if}
        </div>
        <div class="diag-row">
          <span class="diag-label">DayZ installed</span>
          {#if env.dayz_installed}
            <span class="diag-value ok">✓ {env.dayz_path}</span>
          {:else}
            <span class="diag-value bad">
              ✗ not found — set the DayZ install location above, or restart Steam with the
              drive mounted
            </span>
          {/if}
        </div>
        {#if env.dayz_version}
          <div class="diag-row">
            <span class="diag-label">DayZ version</span>
            <span class="diag-value ok">{env.dayz_version}</span>
          </div>
        {/if}
        <div class="diag-row">
          <span class="diag-label">Terminal for login</span>
          {#if env.terminal}
            <span class="diag-value ok">✓ {env.terminal}</span>
          {:else}
            <span class="diag-value bad">✗ none found</span>
          {/if}
        </div>
        <div class="diag-row">
          <span class="diag-label">Steam account</span>
          {#if env.steam_login && env.steam_login !== "anonymous"}
            <span class="diag-value ok">✓ {env.steam_login}</span>
          {:else}
            <span class="diag-value bad">✗ not set</span>
          {/if}
        </div>
      </div>
      <p class="hint">Actual Steam login is verified when you Play.</p>
    {:else}
      <p class="hint">Checking environment…</p>
    {/if}
  </div>
{/if}
