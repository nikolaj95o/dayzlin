<script lang="ts">
  import type { Server } from "./api";

  let {
    servers = [],
    onSelect,
    isFavorite = () => false,
    onToggleFavorite = () => {},
  }: {
    servers: Server[];
    onSelect: (s: Server) => void;
    isFavorite?: (s: Server) => boolean;
    onToggleFavorite?: (s: Server) => void;
  } = $props();

  // Row virtualization: only the rows in (and just around) the viewport are in the DOM, so the
  // table stays cheap to render even with thousands of servers. Relies on a fixed row height,
  // enforced in CSS (`tbody tr { height: 44px }`).
  const ROW_H = 44;
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
    { key: "name", label: "Name" },
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
  class="min-h-0 flex-1 overflow-auto"
  bind:this={scrollEl}
  bind:clientHeight={viewH}
  onscroll={() => (scrollTop = scrollEl?.scrollTop ?? 0)}
>
<table class="w-full border-collapse text-sm">
  <thead>
    <tr>
      {#each columns as col}
        {@const info = sortInfo(col.key)}
        <th class="sticky top-0 border-b border-border bg-bg p-0 text-left font-semibold text-text-h" aria-sort={info ? (info.dir === "asc" ? "ascending" : "descending") : "none"}>
          <!-- Header sort buttons: look like header text, not default buttons. -->
          <button class="flex w-full cursor-pointer items-center gap-1 border-0 bg-transparent px-2.5 py-2 text-left font-[inherit] text-text-h" onclick={() => toggleSort(col.key)}>
            {col.label}
            <!-- Reserve a fixed slot for the arrow so toggling it doesn't shift the header row. -->
            <span class="inline-block w-[0.9em] text-[0.7em] leading-none">{info ? (info.dir === "asc" ? "▲" : "▼") : ""}</span>
            {#if info && sortChain.length > 1}<span class="text-[0.7em] leading-none text-accent">{info.pos}</span>{/if}
          </button>
        </th>
      {/each}
      <th class="sticky top-0 border-b border-border bg-bg px-2 py-1 text-right">
        {#if sortChain.length > 0}
          <button
            class="inline-flex h-[22px] w-[22px] cursor-pointer items-center justify-center rounded-md border border-border bg-transparent p-0 text-text hover:border-accent-border hover:text-text-h"
            title="Clear sorting"
            aria-label="Clear sorting"
            onclick={() => (sortChain = [])}
          >
            <svg class="h-4 w-4 fill-none stroke-current [stroke-linecap:round] [stroke-width:1.5]" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
          </button>
        {/if}
      </th>
    </tr>
  </thead>
  <tbody>
    {#if sorted.length === 0}
      <tr><td colspan="6" class="p-6 text-center text-text">No servers</td></tr>
    {:else}
      {#if start > 0}
        <!-- Height must sit on a cell: WebKit ignores `height` on a cell-less <tr>. -->
        <tr aria-hidden="true"><td colspan="6" style="height:{start * ROW_H}px;padding:0;border:0"></td></tr>
      {/if}
      {#each slice as s (s.ip + ":" + s.game_port)}
      <!-- Fixed 44px row height (h-11) is required by the virtualization math (ROW_H = 44). -->
      <tr class="box-border h-11 hover:bg-bg-alt">
        <td class="max-w-[380px] overflow-hidden border-b border-border px-2.5 py-[7px] font-medium text-ellipsis whitespace-nowrap text-text-h">{s.name}</td>
        <td class="border-b border-border px-2.5 py-[7px]">{s.map}</td>
        <td class="border-b border-border px-2.5 py-[7px]">{s.players}/{s.max_players}</td>
        <td class="border-b border-border px-2.5 py-[7px]">{s.mods.length}</td>
        <td class="border-b border-border px-2.5 py-[7px]">{s.first_person ? "1PP" : "3PP"}</td>
        <td class="flex items-center gap-1.5 border-b border-border px-2.5 py-[7px] whitespace-nowrap">
          {#if s.version_match === false}
            <!-- Shown in place of Play when the server's build doesn't match the installed DayZ. -->
            <span class="inline-flex h-[30px] items-center rounded-[4px] border border-border px-2 text-[0.7em] font-semibold tracking-[0.03em] whitespace-nowrap text-text opacity-75" title="Server build {s.version} differs from your installed DayZ">NOT SAME VER</span>
          {:else}
            <button class="icon-btn-accent" title="Play" aria-label="Play" onclick={() => onSelect(s)}>
              <svg class="h-4 w-4 fill-current" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.5v11l9-5.5z" /></svg>
            </button>
          {/if}
          <button
            class="icon-btn {isFavorite(s) ? 'border-accent-border text-accent' : ''}"
            title={isFavorite(s) ? "Remove from favorites" : "Add to favorites"}
            aria-label={isFavorite(s) ? "Remove from favorites" : "Add to favorites"}
            aria-pressed={isFavorite(s)}
            onclick={() => onToggleFavorite(s)}
          >
            <svg class="h-4 w-4 stroke-current [stroke-linejoin:round] [stroke-width:1.2] {isFavorite(s) ? 'fill-current' : 'fill-none'}" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M8 1.5l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.8 4.2 13.8l.7-4.3-3.1-3 4.3-.6z"
              />
            </svg>
          </button>
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
