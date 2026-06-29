<script lang="ts">
  import { getProfile, saveProfile, type Profile } from "./api";

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
  </div>
{/if}
