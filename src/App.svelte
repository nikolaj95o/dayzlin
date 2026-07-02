<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    listServers,
    filterServers,
    play,
    getProfile,
    toggleFavorite,
    listInstalledMods,
    deleteInstalledMod,
    type Server,
    type ServerRef,
    type ServerFilter,
    type Profile,
    type InstalledMod,
    type LaunchProgress,
  } from "./lib/api";
  import ServerTable from "./lib/ServerTable.svelte";
  import ModsTable from "./lib/ModsTable.svelte";
  import FilterPanel from "./lib/FilterPanel.svelte";
  import Settings from "./lib/Settings.svelte";
  import MessageDialog from "./lib/MessageDialog.svelte";
  import LaunchDialog from "./lib/LaunchDialog.svelte";
  import { showError } from "./lib/dialog";
  import { updateAvailable, refreshUpdateStatus } from "./lib/update.svelte";
  import { startLaunch, setLaunch, closeLaunch } from "./lib/launch";
  import { Button } from "$lib/components/ui/button/index.js";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Minus from "@lucide/svelte/icons/minus";
  import Square from "@lucide/svelte/icons/square";
  import Copy from "@lucide/svelte/icons/copy";
  import X from "@lucide/svelte/icons/x";

  // Native decorations are disabled (tauri.conf.json) so we draw our own titlebar; the
  // header below is the drag region and hosts the min/max/close controls.
  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);

  type View = "servers" | "favorites" | "history" | "mods" | "settings";

  let view = $state<View>("servers");
  let servers = $state<Server[]>([]);
  let mods = $state<InstalledMod[]>([]);
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

  // How many favorite servers use each mod, keyed by workshop id. Built from the resolved live
  // favorites, so a favorite that isn't in the current server list (offline) has no known mods and
  // can't contribute — the same limitation the Favorites tab already has.
  const favoriteModCounts = $derived.by(() => {
    const counts = new Map<number, number>();
    for (const s of favoriteServers)
      for (const m of s.mods)
        counts.set(m.workshop_id, (counts.get(m.workshop_id) ?? 0) + 1);
    return counts;
  });
  const favoriteCount = (id: number) => favoriteModCounts.get(id) ?? 0;

  async function loadMods() {
    try {
      mods = await listInstalledMods();
    } catch (e) {
      showError(e);
    }
  }

  async function onDeleteMod(m: InstalledMod) {
    try {
      await deleteInstalledMod(m.workshop_id);
      await loadMods();
    } catch (e) {
      showError(e);
    }
  }

  // Unique map names for the Map filter dropdown, drawn from the full cached list so options
  // don't disappear as other filters narrow the visible results. Lowercased and deduped so
  // maps differing only in case collapse to one option (the backend matches case-insensitively).
  const mapOptions = $derived(
    [...new Set(allServers.map((s) => s.map.toLowerCase()).filter(Boolean))].sort(),
  );

  async function refreshProfile() {
    profile = await getProfile();
  }

  function show(v: View) {
    view = v;
    if (v === "favorites" || v === "history") refreshProfile();
    // The Mods tab needs a fresh scan and up-to-date favorites for the "used by" count.
    if (v === "mods") {
      loadMods();
      refreshProfile();
    }
  }

  onMount(() => {
    // Stale-while-revalidate: render cached servers instantly, then refresh in the background
    // only when the cache is stale (older than the TTL) or missing.
    load(false).then((stale) => {
      if (stale) load(true, true);
    });
    refreshProfile();
    // Quiet, best-effort: lights the Settings-tab dot if a newer release exists.
    refreshUpdateStatus();
    const un = listen<LaunchProgress>("launch-progress", (e) => {
      setLaunch(e.payload);
    });
    // Keep the maximize/restore icon in sync with the actual window state (maximizing via
    // double-click on the drag region, keyboard, or the compositor all fire onResized).
    appWindow.isMaximized().then((m) => (isMaximized = m));
    const unResize = appWindow.onResized(() => {
      appWindow.isMaximized().then((m) => (isMaximized = m));
    });
    return () => {
      un.then((f) => f());
      unResize.then((f) => f());
    };
  });
</script>

<!-- Full-width column so the custom titlebar (and its window controls) spans the whole window;
     the body below stays centered in the max-w-6xl content column. -->
<div class="flex h-screen flex-col">
  <header data-tauri-drag-region class="flex items-center gap-4 px-4 pt-4 sm:px-5">
    <h1 class="mt-2 mb-4 text-3xl font-semibold">dayzlin</h1>
    <nav class="ml-auto flex gap-1.5">
      <Button variant={view === "servers" ? "secondary" : "ghost"} size="sm" onclick={() => show("servers")}>Servers</Button>
      <Button variant={view === "favorites" ? "secondary" : "ghost"} size="sm" onclick={() => show("favorites")}>Favorites</Button>
      <Button variant={view === "history" ? "secondary" : "ghost"} size="sm" onclick={() => show("history")}>History</Button>
      <Button variant={view === "mods" ? "secondary" : "ghost"} size="sm" onclick={() => show("mods")}>Mods</Button>
      <Button variant={view === "settings" ? "secondary" : "ghost"} size="sm" onclick={() => show("settings")}>
        Settings
        {#if updateAvailable()}<span class="ml-1 inline-block h-1.5 w-1.5 rounded-full bg-amber-500 align-middle" title="Update available"></span>{/if}
      </Button>
    </nav>
    <div class="flex gap-1">
      <Button variant="ghost" size="icon-sm" aria-label="Minimize" onclick={() => appWindow.minimize()}>
        <Minus />
      </Button>
      <Button variant="ghost" size="icon-sm" aria-label={isMaximized ? "Restore" : "Maximize"} onclick={() => appWindow.toggleMaximize()}>
        {#if isMaximized}<Copy />{:else}<Square />{/if}
      </Button>
      <Button variant="ghost" size="icon-sm" aria-label="Close" class="hover:bg-destructive hover:text-white" onclick={() => appWindow.close()}>
        <X />
      </Button>
    </div>
  </header>

  <main class="mx-auto flex min-h-0 w-full max-w-6xl flex-1 flex-col box-border px-4 pb-12 sm:px-5">
  <p class="text-muted-foreground my-2 font-mono text-sm">{status}</p>

  <!-- Keep the Servers view mounted and hide it with CSS so switching tabs doesn't destroy and
       rebuild the (virtualized) table — returning to it is instant and scroll position survives. -->
  <div class="flex min-h-0 flex-1 flex-col" class:hidden={view !== "servers"}>
    <div class="flex flex-wrap items-center gap-3">
      <Button variant="outline" size="sm" onclick={() => load(true)}>Refresh</Button>
      <FilterPanel bind:filter bind:query {mapOptions} onChange={applyFilters} />
    </div>
    <ServerTable {servers} {onSelect} {isFavorite} {onToggleFavorite} />
  </div>

  {#if view === "settings"}
    <Settings />
  {:else if view === "favorites"}
    <ServerTable servers={favoriteServers} {onSelect} {isFavorite} {onToggleFavorite} {isOffline} emptyLabel="No favorites yet" />
  {:else if view === "history"}
    <ServerTable servers={historyServers} {onSelect} {isFavorite} {onToggleFavorite} {isOffline} emptyLabel="No history yet" />
  {:else if view === "mods"}
    <ModsTable {mods} {favoriteCount} onDelete={onDeleteMod} emptyLabel="No installed mods" />
  {/if}
  </main>

  <MessageDialog />
  <LaunchDialog />
</div>
