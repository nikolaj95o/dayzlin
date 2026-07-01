<script lang="ts">
  import { openWorkshopPage, openModFolder, type InstalledMod } from "./api";
  import { showError } from "./dialog";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Trash2 from "@lucide/svelte/icons/trash-2";

  let {
    mods = [],
    favoriteCount = () => 0,
    onDelete,
    emptyLabel = "No installed mods",
  }: {
    mods: InstalledMod[];
    // How many of the user's favorite servers use a given mod (looked up by workshop id).
    favoriteCount?: (id: number) => number;
    onDelete: (m: InstalledMod) => void;
    emptyLabel?: string;
  } = $props();

  // Click-to-sort, mirroring ServerTable: each column maps to a comparable value; strings compare
  // via localeCompare, numbers via subtraction. The list is small (dozens–hundreds) so, unlike the
  // server list, there's no virtualization — every row is rendered.
  type SortKey = "name" | "last_used" | "favorites" | "size";
  const sortValue: Record<SortKey, (m: InstalledMod) => string | number> = {
    name: (m) => m.name,
    last_used: (m) => m.last_used ?? 0,
    favorites: (m) => favoriteCount(m.workshop_id),
    size: (m) => m.size_bytes,
  };
  const columns: { key: SortKey; label: string }[] = [
    { key: "name", label: "Name" },
    { key: "last_used", label: "Last used" },
    { key: "favorites", label: "Fav servers" },
    { key: "size", label: "Size" },
  ];

  // Sort chain: array order is priority (index 0 = primary). Each header click appends a column,
  // then cycles asc → desc → removed on repeat clicks.
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

  function sortInfo(key: SortKey): { pos: number; dir: "asc" | "desc" } | null {
    const i = sortChain.findIndex((c) => c.key === key);
    return i === -1 ? null : { pos: i + 1, dir: sortChain[i].dir };
  }

  const sorted = $derived.by(() => {
    if (sortChain.length === 0) return mods;
    return mods.slice().sort((a, b) => {
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

  const totalSize = $derived(mods.reduce((sum, m) => sum + m.size_bytes, 0));

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    const units = ["KB", "MB", "GB", "TB"];
    let v = n / 1024;
    let i = 0;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i++;
    }
    // Whole numbers below 10 look odd with a trailing .0, so only keep a decimal for 1.x–9.x.
    return `${v >= 10 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
  }

  function formatDate(secs: number | null): string {
    return secs == null ? "—" : new Date(secs * 1000).toLocaleDateString();
  }

  // A single confirm dialog, driven by which mod (if any) is pending deletion.
  let pendingDelete = $state<InstalledMod | null>(null);

  async function openWorkshop(m: InstalledMod) {
    try {
      await openWorkshopPage(m.workshop_id);
    } catch (e) {
      showError(e);
    }
  }
  async function openFolder(m: InstalledMod) {
    try {
      await openModFolder(m.workshop_id);
    } catch (e) {
      showError(e);
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <p class="text-muted-foreground my-2 text-sm">
    {mods.length} mod{mods.length === 1 ? "" : "s"} · {formatBytes(totalSize)}
  </p>
  <div class="min-h-0 flex-1 overflow-auto">
    <table class="w-full border-collapse text-sm">
      <thead>
        <tr>
          {#each columns as col}
            {@const info = sortInfo(col.key)}
            <th class="border-border bg-background text-foreground sticky top-0 border-b p-0 text-left font-semibold" aria-sort={info ? (info.dir === "asc" ? "ascending" : "descending") : "none"}>
              <button class="text-foreground flex w-full cursor-pointer items-center gap-1 px-2.5 py-2 text-left font-[inherit]" onclick={() => toggleSort(col.key)}>
                {col.label}
                <span class="inline-block w-[0.9em] text-[0.7em] leading-none">{info ? (info.dir === "asc" ? "▲" : "▼") : ""}</span>
                {#if info && sortChain.length > 1}<span class="text-primary text-[0.7em] leading-none">{info.pos}</span>{/if}
              </button>
            </th>
          {/each}
          <th class="border-border bg-background sticky top-0 border-b px-2 py-1 text-right">
            {#if sortChain.length > 0}
              <Button variant="ghost" size="icon-xs" title="Clear sorting" aria-label="Clear sorting" onclick={() => (sortChain = [])}>
                <span aria-hidden="true">✕</span>
              </Button>
            {/if}
          </th>
        </tr>
      </thead>
      <tbody>
        {#if sorted.length === 0}
          <tr><td colspan="5" class="text-muted-foreground p-6 text-center">{emptyLabel}</td></tr>
        {:else}
          {#each sorted as m (m.workshop_id)}
            <tr class="hover:bg-muted box-border h-11">
              <td class="border-border text-foreground max-w-[420px] overflow-hidden border-b px-2.5 py-[7px] font-medium text-ellipsis whitespace-nowrap">{m.name}</td>
              <td class="border-border border-b px-2.5 py-[7px] whitespace-nowrap">{formatDate(m.last_used)}</td>
              <td class="border-border border-b px-2.5 py-[7px]">{favoriteCount(m.workshop_id)}</td>
              <td class="border-border border-b px-2.5 py-[7px] whitespace-nowrap">{formatBytes(m.size_bytes)}</td>
              <td class="border-border flex items-center justify-end gap-1.5 border-b px-2.5 py-[7px] whitespace-nowrap">
                <Button variant="outline" size="icon-sm" title="Open Steam Workshop page" aria-label="Open Steam Workshop page" onclick={() => openWorkshop(m)}>
                  <ExternalLink />
                </Button>
                <Button variant="outline" size="icon-sm" title="Open mod folder" aria-label="Open mod folder" onclick={() => openFolder(m)}>
                  <FolderOpen />
                </Button>
                <Button variant="outline" size="icon-sm" title="Delete mod" aria-label="Delete mod" onclick={() => (pendingDelete = m)}>
                  <Trash2 />
                </Button>
              </td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>
  </div>
</div>

<AlertDialog.Root open={pendingDelete !== null} onOpenChange={(o) => { if (!o) pendingDelete = null; }}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>Delete this mod?</AlertDialog.Title>
      <AlertDialog.Description>
        This deletes “{pendingDelete?.name}” ({formatBytes(pendingDelete?.size_bytes ?? 0)}) from disk.
        You'll need to re-download it before joining a server that uses it.
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action onclick={() => { if (pendingDelete) onDelete(pendingDelete); pendingDelete = null; }}>
        Delete
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
