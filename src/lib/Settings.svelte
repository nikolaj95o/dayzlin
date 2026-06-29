<script lang="ts">
  import {
    getProfile,
    saveProfile,
    setupSteamLogin,
    type Profile,
  } from "./api";
  import { showError, showMessage } from "./dialog";

  let profile = $state<Profile | null>(null);
  let saved = $state(false);

  async function loadP() {
    profile = await getProfile();
  }
  async function saveP() {
    if (!profile) return;
    await saveProfile(profile);
    saved = true;
    setTimeout(() => (saved = false), 1500);
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
  </div>
{/if}
