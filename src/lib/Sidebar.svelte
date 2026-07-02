<script lang="ts" module>
  export type View = "servers" | "favorites" | "history" | "mods" | "settings";
</script>

<script lang="ts">
  import List from "@lucide/svelte/icons/list";
  import Star from "@lucide/svelte/icons/star";
  import History from "@lucide/svelte/icons/history";
  import Package from "@lucide/svelte/icons/package";
  import Settings from "@lucide/svelte/icons/settings";
  import type { Component } from "svelte";

  let {
    view,
    onSelect,
    updateAvailable = false,
    status = "",
  }: {
    view: View;
    onSelect: (v: View) => void;
    updateAvailable?: boolean;
    status?: string;
  } = $props();

  const items: { key: View; label: string; icon: Component }[] = [
    { key: "servers", label: "Servers", icon: List },
    { key: "favorites", label: "Favorites", icon: Star },
    { key: "history", label: "History", icon: History },
    { key: "mods", label: "Mods", icon: Package },
    { key: "settings", label: "Settings", icon: Settings },
  ];
</script>

<aside class="border-sidebar-border bg-sidebar flex w-52 shrink-0 flex-col border-r">
  <nav class="flex flex-col gap-0.5 p-2.5">
    {#each items as item (item.key)}
      {@const active = view === item.key}
      <button
        type="button"
        onclick={() => onSelect(item.key)}
        aria-current={active ? "page" : undefined}
        class="group relative flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm font-medium transition-colors
          {active
            ? 'bg-sidebar-accent text-sidebar-accent-foreground'
            : 'text-sidebar-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground'}"
      >
        <!-- Amber signal bar on the active item. -->
        <span
          class="bg-primary absolute top-1/2 left-0 h-5 w-[3px] -translate-y-1/2 rounded-r-full transition-opacity
            {active ? 'opacity-100' : 'opacity-0'}"
          aria-hidden="true"
        ></span>
        <item.icon class="size-4 shrink-0 {active ? 'text-primary' : 'text-muted-foreground group-hover:text-foreground'}" />
        <span class="font-display tracking-wide uppercase text-[13px]">{item.label}</span>
        {#if item.key === "settings" && updateAvailable}
          <span class="bg-primary ml-auto size-1.5 rounded-full" title="Update available"></span>
        {/if}
      </button>
    {/each}
  </nav>

  <!-- Status footer: server count / refreshing state, moved out of the content body. -->
  <div class="border-sidebar-border mt-auto flex items-center gap-2 border-t px-3.5 py-3">
    <span class="bg-online size-1.5 shrink-0 rounded-full {status ? '' : 'opacity-40'}" aria-hidden="true"></span>
    <span class="text-muted-foreground truncate font-mono text-xs">{status || "—"}</span>
  </div>
</aside>
