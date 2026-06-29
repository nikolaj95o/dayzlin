<script lang="ts">
  import { onMount } from "svelte";
  import {
    listServers,
    filterServers,
    play,
    type Server,
    type ServerFilter,
  } from "./lib/api";
  import ServerTable from "./lib/ServerTable.svelte";
  import FilterPanel from "./lib/FilterPanel.svelte";

  let servers = $state<Server[]>([]);
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

  onMount(() => load(false));
</script>

<main>
  <header>
    <h1>dayzlin</h1>
    <button onclick={() => load(true)}>Refresh</button>
  </header>
  <FilterPanel bind:filter bind:query onChange={applyFilters} />
  <p class="status">{status}</p>
  <ServerTable {servers} {onSelect} />
</main>
