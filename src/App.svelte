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
  // Unfiltered master list, used to resolve saved favorite/history ServerRefs to full live
  // servers regardless of the Servers-tab filter. Refreshed alongside `servers` in load().
  let allServers = $state<Server[]>([]);
  let profile = $state<Profile | null>(null);
  let query = $state("");
  let filter = $state<ServerFilter>({
    map: null,
    first_person_only: false,
    third_person_only: false,
    no_password: false,
    max_mods: null,
    min_players: null,
    has_slots: false,
    same_version_only: true,
  });
  let status = $state("");

  // Matches every server (no filtering); used to resolve saved favorites/history to live data.
  const ALL_SERVERS: ServerFilter = {
    map: null,
    first_person_only: false,
    third_person_only: false,
    no_password: false,
    max_mods: null,
    min_players: null,
    has_slots: false,
    same_version_only: false,
  };

  async function load(refresh = false, background = false): Promise<boolean> {
    status = background ? "Refreshing…" : "Loading servers…";
    try {
      const stale = await listServers(refresh);
      servers = await filterServers(filter, query);
      // A fully-permissive query for the favorites/history resolver — every cached server.
      allServers = await filterServers(ALL_SERVERS, "");
      status = `${servers.length} servers`;
      return stale;
    } catch (e) {
      status = "";
      showError(e);
      return false;
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

  // Resolve saved ServerRefs to full live servers so Favorites/History can reuse ServerTable.
  // Refs not in the live list become a placeholder server flagged offline (no live columns,
  // no Play) — its real name/ip/port are kept so removing/keying still works.
  const key = (ip: string, port: number) => `${ip}:${port}`;
  const liveByKey = $derived(
    new Map(allServers.map((s) => [key(s.ip, s.game_port), s])),
  );
  function resolve(refs: ServerRef[]): Server[] {
    return refs.map(
      (r) =>
        liveByKey.get(key(r.ip, r.port)) ?? {
          name: r.name,
          ip: r.ip,
          game_port: r.port,
          players: 0,
          max_players: 0,
          map: "",
          time: "",
          first_person: false,
          password: false,
          mods: [],
          version: "",
          version_match: null,
        },
    );
  }
  const favoriteServers = $derived(resolve(profile?.favorites ?? []));
  const historyServers = $derived(resolve(profile?.history ?? []));
  const isOffline = (s: Server) => !liveByKey.has(key(s.ip, s.game_port));

  async function refreshProfile() {
    profile = await getProfile();
  }

  function show(v: View) {
    view = v;
    if (v === "favorites" || v === "history") refreshProfile();
  }

  onMount(() => {
    // Stale-while-revalidate: render cached servers instantly, then refresh in the background
    // only when the cache is stale (older than the TTL) or missing.
    load(false).then((stale) => {
      if (stale) load(true, true);
    });
    refreshProfile();
    const un = listen<LaunchProgress>("launch-progress", (e) => {
      setLaunch(e.payload);
    });
    return () => {
      un.then((f) => f());
    };
  });
</script>

<main class="mx-auto flex h-screen w-full max-w-[1100px] flex-col box-border px-4 pt-4 pb-12 sm:px-5">
  <header class="flex flex-wrap items-center gap-4">
    <h1 class="mt-2 mb-4 text-[28px] font-semibold text-text-h">dayzlin</h1>
    <nav class="ml-auto flex gap-1.5">
      <button class="btn" class:btn-active={view === "servers"} onclick={() => show("servers")}>Servers</button>
      <button class="btn" class:btn-active={view === "favorites"} onclick={() => show("favorites")}>Favorites</button>
      <button class="btn" class:btn-active={view === "history"} onclick={() => show("history")}>History</button>
      <button class="btn" class:btn-active={view === "settings"} onclick={() => show("settings")}>Settings</button>
    </nav>
  </header>

  <p class="my-2 font-mono text-[13px] text-text">{status}</p>

  <!-- Keep the Servers view mounted and hide it with CSS so switching tabs doesn't destroy and
       rebuild the (virtualized) table — returning to it is instant and scroll position survives. -->
  <div class="flex min-h-0 flex-1 flex-col" class:hidden={view !== "servers"}>
    <div class="flex flex-wrap items-center gap-3">
      <button class="btn" onclick={() => load(true)}>Refresh</button>
      <FilterPanel bind:filter bind:query onChange={applyFilters} />
    </div>
    <ServerTable {servers} {onSelect} {isFavorite} {onToggleFavorite} />
  </div>

  {#if view === "settings"}
    <Settings />
  {:else if view === "favorites"}
    <ServerTable servers={favoriteServers} {onSelect} {isFavorite} {onToggleFavorite} {isOffline} emptyLabel="No favorites yet" />
  {:else if view === "history"}
    <ServerTable servers={historyServers} {onSelect} {isFavorite} {onToggleFavorite} {isOffline} emptyLabel="No history yet" />
  {/if}
</main>

<MessageDialog />
<LaunchDialog />
