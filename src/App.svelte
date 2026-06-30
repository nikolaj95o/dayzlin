<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    listServers,
    filterServers,
    play,
    getProfile,
    toggleFavorite,
    type Server,
    type ServerRef,
    type ServerFilter,
    type Profile,
    type LaunchProgress,
  } from "./lib/api";
  import ServerTable from "./lib/ServerTable.svelte";
  import FilterPanel from "./lib/FilterPanel.svelte";
  import Settings from "./lib/Settings.svelte";
  import MessageDialog from "./lib/MessageDialog.svelte";
  import LaunchDialog from "./lib/LaunchDialog.svelte";
  import { showError } from "./lib/dialog";
  import { startLaunch, setLaunch, closeLaunch } from "./lib/launch";

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
    same_version_only: true,
  });
  let status = $state("");

  async function load(refresh = false, background = false) {
    status = background ? "Refreshing…" : "Loading servers…";
    try {
      await listServers(refresh);
      servers = await filterServers(filter, query);
      status = `${servers.length} servers`;
    } catch (e) {
      status = "";
      showError(e);
    }
  }

  async function runFilter() {
    try {
      servers = await filterServers(filter, query);
      status = `${servers.length} servers`;
    } catch (e) {
      status = "";
      showError(e);
    }
  }

  // Debounce filter changes: only query 1s after the last change (text + checkboxes).
  let filterTimer: ReturnType<typeof setTimeout>;
  function applyFilters() {
    clearTimeout(filterTimer);
    filterTimer = setTimeout(runFilter, 1000);
  }

  async function onSelect(s: Server) {
    // Collect the password before blocking the UI so the prompt isn't hidden.
    const password = s.password ? prompt("Server password") : null;
    status = `Launching ${s.name}…`;
    startLaunch();
    try {
      await play(s, password);
      setLaunch({ phase: "starting" });
      setTimeout(closeLaunch, 3000);
      status = "Launched";
      // Reflect the just-played server in History/Favorites without needing a tab switch.
      refreshProfile();
    } catch (e) {
      closeLaunch();
      status = "";
      // A user-initiated Cancel is not an error worth surfacing.
      if ((e as { kind?: string })?.kind !== "cancelled") showError(e);
    }
  }

  const isFavorite = (s: Server) =>
    profile?.favorites.some((f) => f.ip === s.ip && f.port === s.game_port) ?? false;

  async function onToggleFavorite(s: Server) {
    profile = await toggleFavorite({ name: s.name, ip: s.ip, port: s.game_port });
  }

  // Resolve a saved favorite/history entry back to a full live server, then launch it.
  function playRef(r: ServerRef) {
    const full = servers.find((s) => s.ip === r.ip && s.game_port === r.port);
    if (full) onSelect(full);
    else status = `${r.name} is offline or not in the current list`;
  }

  async function removeFavorite(r: ServerRef) {
    profile = await toggleFavorite(r);
  }

  async function refreshProfile() {
    profile = await getProfile();
  }

  function show(v: View) {
    view = v;
    if (v === "favorites" || v === "history") refreshProfile();
  }

  onMount(() => {
    // Stale-while-revalidate: render cached servers instantly, then refresh in the background.
    load(false).then(() => load(true, true));
    refreshProfile();
    const un = listen<LaunchProgress>("launch-progress", (e) => {
      setLaunch(e.payload);
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

  <!-- Keep the Servers view mounted and hide it with CSS so switching tabs doesn't destroy and
       rebuild the (virtualized) table — returning to it is instant and scroll position survives. -->
  <div class="view" class:hidden={view !== "servers"}>
    <div class="toolbar">
      <button onclick={() => load(true)}>Refresh</button>
      <FilterPanel bind:filter bind:query onChange={applyFilters} />
    </div>
    <ServerTable {servers} {onSelect} {isFavorite} {onToggleFavorite} />
  </div>

  {#if view === "settings"}
    <Settings />
  {:else if view === "favorites" || view === "history"}
    <ul class="refs">
      {#each (view === "favorites" ? profile?.favorites : profile?.history) ?? [] as r}
        <li>
          <span class="name">{r.name}</span>
          <span class="addr">{r.ip}:{r.port}</span>
          <span class="ref-actions">
            <button class="icon play" title="Play" aria-label="Play" onclick={() => playRef(r)}>
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.5v11l9-5.5z" /></svg>
            </button>
            {#if view === "favorites"}
              <button
                class="icon remove"
                title="Remove from favorites"
                aria-label="Remove from favorites"
                onclick={() => removeFavorite(r)}
              >
                <svg viewBox="0 0 16 16" aria-hidden="true">
                  <path d="M4 4l8 8M12 4l-8 8" />
                </svg>
              </button>
            {/if}
          </span>
        </li>
      {:else}
        <li class="empty">No {view} yet</li>
      {/each}
    </ul>
  {/if}
</main>

<MessageDialog />
<LaunchDialog />
