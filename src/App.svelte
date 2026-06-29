<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    listServers,
    filterServers,
    play,
    getProfile,
    type Server,
    type ServerFilter,
    type Profile,
  } from "./lib/api";
  import ServerTable from "./lib/ServerTable.svelte";
  import FilterPanel from "./lib/FilterPanel.svelte";
  import Settings from "./lib/Settings.svelte";

  type View = "servers" | "favorites" | "history" | "settings";

  let view = $state<View>("servers");
  let servers = $state<Server[]>([]);
  let profile = $state<Profile | null>(null);
  let query = $state("");
  let filter = $state<ServerFilter>({
    map: null,
    first_person_only: false,
    no_password: false,
    max_mods: null,
    min_players: null,
    has_slots: false,
  });
  let status = $state("");

  async function load(refresh = false) {
    status = "Loading servers…";
    try {
      await listServers(refresh);
      servers = await filterServers(filter, query);
      status = `${servers.length} servers`;
    } catch (e) {
      status = `Error: ${e}`;
    }
  }

  async function applyFilters() {
    try {
      servers = await filterServers(filter, query);
      status = `${servers.length} servers`;
    } catch (e) {
      status = `Error: ${e}`;
    }
  }

  async function onSelect(s: Server) {
    status = `Launching ${s.name}…`;
    try {
      await play(s, s.password ? prompt("Server password") : null);
      status = "Launched";
    } catch (e) {
      status = `Error: ${e}`;
    }
  }

  async function refreshProfile() {
    profile = await getProfile();
  }

  function show(v: View) {
    view = v;
    if (v === "favorites" || v === "history") refreshProfile();
  }

  onMount(() => {
    load(false);
    refreshProfile();
    const un = listen<string>("mod-progress", (e) => {
      status = e.payload;
    });
    return () => {
      un.then((f) => f());
    };
  });
</script>

<main>
  <header>
    <h1>dayzlin</h1>
    <nav>
      <button class:active={view === "servers"} onclick={() => show("servers")}>Servers</button>
      <button class:active={view === "favorites"} onclick={() => show("favorites")}>Favorites</button>
      <button class:active={view === "history"} onclick={() => show("history")}>History</button>
      <button class:active={view === "settings"} onclick={() => show("settings")}>Settings</button>
    </nav>
  </header>

  <p class="status">{status}</p>

  {#if view === "servers"}
    <div class="toolbar">
      <button onclick={() => load(true)}>Refresh</button>
      <FilterPanel bind:filter bind:query onChange={applyFilters} />
    </div>
    <ServerTable {servers} {onSelect} />
  {:else if view === "settings"}
    <Settings />
  {:else}
    <ul class="refs">
      {#each (view === "favorites" ? profile?.favorites : profile?.history) ?? [] as r}
        <li>
          <span class="name">{r.name}</span>
          <span class="addr">{r.ip}:{r.port}</span>
        </li>
      {:else}
        <li class="empty">No {view} yet</li>
      {/each}
    </ul>
  {/if}
</main>
