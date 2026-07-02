<script lang="ts">
  import type { Server } from "./api";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import FillBar from "./FillBar.svelte";
  import Play from "@lucide/svelte/icons/play";
  import Star from "@lucide/svelte/icons/star";
  import Lock from "@lucide/svelte/icons/lock";
  import Package from "@lucide/svelte/icons/package";
  import ChevronUp from "@lucide/svelte/icons/chevron-up";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import X from "@lucide/svelte/icons/x";

  let {
    servers = [],
    onSelect,
    isFavorite = () => false,
    onToggleFavorite = () => {},
    isOffline = () => false,
    emptyLabel = "No servers",
  }: {
    servers: Server[];
    onSelect: (s: Server) => void;
    isFavorite?: (s: Server) => boolean;
    onToggleFavorite?: (s: Server) => void;
    // Marks a row whose live data couldn't be resolved (saved server not currently online):
    // its columns render as "—" and Play is replaced by an OFFLINE badge.
    isOffline?: (s: Server) => boolean;
    emptyLabel?: string;
  } = $props();

  // Row virtualization: only the rows in (and just around) the viewport are in the DOM, so the
  // table stays cheap to render even with thousands of servers. Relies on a fixed row height,
  // enforced in CSS (`tbody tr { height: 48px }` via `h-12`).
  const ROW_H = 48;
  const OVERSCAN = 8;

  let scrollEl = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewH = $state(0);

  // Click-to-sort. Each column maps to a comparable value; strings compare via localeCompare,
  // numbers/booleans via subtraction. `sortKey === null` keeps the feed's original order.
  type SortKey = "name" | "map" | "players" | "mods" | "view";
  const sortValue: Record<SortKey, (s: Server) => string | number> = {
    name: (s) => s.name,
    map: (s) => s.map,
    players: (s) => s.players,
    mods: (s) => s.mods.length,
    view: (s) => (s.first_person ? 1 : 0),
  };
  const columns: { key: SortKey; label: string }[] = [
    { key: "name", label: "Server" },
    { key: "map", label: "Map" },
    { key: "players", label: "Players" },
    { key: "mods", label: "Mods" },
    { key: "view", label: "View" },
  ];

  // The sort chain: array order is priority (index 0 = primary sort key). Each click on a header
  // accumulates: a new column is appended, then cycles asc → desc → removed on repeat clicks.
  type SortEntry = { key: SortKey; dir: "asc" | "desc" };
  let sortChain = $state<SortEntry[]>([]);

  function toggleSort(key: SortKey) {
    const i = sortChain.findIndex((c) => c.key === key);
    if (i === -1) {
      sortChain = [...sortChain, { key, dir: "asc" }];
    } else if (sortChain[i].dir === "asc") {
      sortChain = sortChain.map((c, j) => (j === i ? { key, dir: "desc" } : c));
    } else {
      sortChain = sortChain.filter((_, j) => j !== i);
    }
  }

  // Position (1-based) and direction of a column in the chain, or null when it isn't sorted.
  function sortInfo(key: SortKey): { pos: number; dir: "asc" | "desc" } | null {
    const i = sortChain.findIndex((c) => c.key === key);
    return i === -1 ? null : { pos: i + 1, dir: sortChain[i].dir };
  }

  const sorted = $derived.by(() => {
    if (sortChain.length === 0) return servers;
    return servers.slice().sort((a, b) => {
      for (const { key, dir } of sortChain) {
        const av = sortValue[key](a);
        const bv = sortValue[key](b);
        const cmp =
          typeof av === "string" && typeof bv === "string"
            ? av.localeCompare(bv)
            : (av as number) - (bv as number);
        if (cmp !== 0) return dir === "asc" ? cmp : -cmp;
      }
      return 0;
    });
  });

  // When hidden (display:none) the container measures 0; assume a screenful so the first reveal
  // isn't blank before the ResizeObserver behind bind:clientHeight reports the real height.
  const effViewH = $derived(viewH || 600);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const visible = $derived(Math.ceil(effViewH / ROW_H) + 2 * OVERSCAN);
  const end = $derived(Math.min(sorted.length, start + visible));
  const slice = $derived(sorted.slice(start, end));

  // A new filter result or re-sort is a new array — jump back to the top of the list.
  $effect(() => {
    sorted;
    if (scrollEl) scrollEl.scrollTop = 0;
  });
</script>

<div
  class="border-border bg-card/40 mt-2 min-h-0 flex-1 overflow-auto rounded-lg border"
  bind:this={scrollEl}
  bind:clientHeight={viewH}
  onscroll={() => (scrollTop = scrollEl?.scrollTop ?? 0)}
>
<table class="w-full border-collapse text-sm">
  <thead>
    <tr>
      {#each columns as col}
        {@const info = sortInfo(col.key)}
        <th class="border-border bg-card sticky top-0 z-10 border-b p-0 text-left" aria-sort={info ? (info.dir === "asc" ? "ascending" : "descending") : "none"}>
          <!-- Header sort buttons: display-font labels, amber when active. -->
          <button
            class="hover:text-foreground flex w-full cursor-pointer items-center gap-1 px-3 py-2.5 text-left transition-colors {info ? 'text-primary' : 'text-muted-foreground'}"
            onclick={() => toggleSort(col.key)}
          >
            <span class="font-display text-[11px] font-semibold tracking-wider uppercase">{col.label}</span>
            <!-- Reserve a fixed slot for the arrow so toggling it doesn't shift the header row. -->
            <span class="inline-flex w-3 justify-center">
              {#if info}{#if info.dir === "asc"}<ChevronUp class="size-3" />{:else}<ChevronDown class="size-3" />{/if}{/if}
            </span>
            {#if info && sortChain.length > 1}<span class="text-primary text-[10px] font-bold leading-none">{info.pos}</span>{/if}
          </button>
        </th>
      {/each}
      <th class="border-border bg-card sticky top-0 z-10 border-b px-2 py-1 text-right">
        {#if sortChain.length > 0}
          <Button
            variant="ghost"
            size="icon-xs"
            title="Clear sorting"
            aria-label="Clear sorting"
            onclick={() => (sortChain = [])}
          >
            <X />
          </Button>
        {/if}
      </th>
    </tr>
  </thead>
  <tbody>
    {#if sorted.length === 0}
      <tr><td colspan="6" class="text-muted-foreground p-10 text-center">{emptyLabel}</td></tr>
    {:else}
      {#if start > 0}
        <!-- Height must sit on a cell: WebKit ignores `height` on a cell-less <tr>. -->
        <tr aria-hidden="true"><td colspan="6" style="height:{start * ROW_H}px;padding:0;border:0"></td></tr>
      {/if}
      {#each slice as s (s.ip + ":" + s.game_port)}
      {@const offline = isOffline(s)}
      <!-- Fixed 48px row height (h-12) is required by the virtualization math (ROW_H = 48). -->
      <tr class="hover:bg-accent/60 group box-border h-12 transition-colors {offline ? 'opacity-45' : ''}">
        <td class="border-border max-w-[360px] border-b py-1.5 pr-3 pl-3">
          <div class="flex items-center gap-1.5">
            <span class="bg-primary/0 group-hover:bg-primary h-4 w-[2px] shrink-0 rounded-full transition-colors" aria-hidden="true"></span>
            {#if s.password}<Lock class="text-muted-foreground size-3 shrink-0" aria-label="Password protected" />{/if}
            <span class="text-foreground truncate font-medium">{s.name}</span>
          </div>
        </td>
        <td class="border-border text-muted-foreground max-w-[160px] truncate border-b px-3 py-1.5">{offline ? "—" : s.map || "—"}</td>
        <td class="border-border border-b px-3 py-1.5">
          {#if offline}
            <span class="text-muted-foreground">—</span>
          {:else}
            <div class="flex items-center gap-2">
              <span class="text-foreground font-mono text-xs tabular-nums">{s.players}/{s.max_players}</span>
              <FillBar value={s.players} max={s.max_players} />
            </div>
          {/if}
        </td>
        <td class="border-border border-b px-3 py-1.5">
          {#if offline}
            <span class="text-muted-foreground">—</span>
          {:else}
            <span class="text-muted-foreground inline-flex items-center gap-1.5">
              <Package class="size-3.5" />
              <span class="text-foreground font-mono text-xs tabular-nums">{s.mods.length}</span>
            </span>
          {/if}
        </td>
        <td class="border-border border-b px-3 py-1.5">
          {#if offline}<span class="text-muted-foreground">—</span>{:else}<Badge variant="muted" class="font-mono">{s.first_person ? "1PP" : "3PP"}</Badge>{/if}
        </td>
        <td class="border-border flex h-12 items-center justify-end gap-1.5 border-b px-3 py-1.5 whitespace-nowrap">
          {#if offline}
            <!-- Saved server that isn't in the current live list; can't be launched. -->
            <Badge variant="muted" title="Server is offline or not in the current list">OFFLINE</Badge>
          {:else if s.version_match === false}
            <!-- Shown in place of Play when the server's build doesn't match the installed DayZ. -->
            <Badge variant="warn" title="Server build {s.version} differs from your installed DayZ">VER ✗</Badge>
          {:else}
            <Button variant="accent" size="icon-sm" title="Play" aria-label="Play" onclick={() => onSelect(s)}>
              <Play class="fill-current" />
            </Button>
          {/if}
          <Button
            variant="ghost"
            size="icon-sm"
            title={isFavorite(s) ? "Remove from favorites" : "Add to favorites"}
            aria-label={isFavorite(s) ? "Remove from favorites" : "Add to favorites"}
            aria-pressed={isFavorite(s)}
            onclick={() => onToggleFavorite(s)}
          >
            <Star class={isFavorite(s) ? "fill-primary text-primary" : "text-muted-foreground"} />
          </Button>
        </td>
      </tr>
      {/each}
      {#if end < sorted.length}
        <tr aria-hidden="true"><td colspan="6" style="height:{(sorted.length - end) * ROW_H}px;padding:0;border:0"></td></tr>
      {/if}
    {/if}
  </tbody>
</table>
</div>
