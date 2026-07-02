<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Button } from "$lib/components/ui/button/index.js";
  import Minus from "@lucide/svelte/icons/minus";
  import Square from "@lucide/svelte/icons/square";
  import Copy from "@lucide/svelte/icons/copy";
  import X from "@lucide/svelte/icons/x";

  // Native decorations are disabled (tauri.conf.json) so we draw our own titlebar; this bar is the
  // window drag region and hosts the min/max/close controls plus the wordmark.
  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);

  onMount(() => {
    // Keep the maximize/restore icon in sync with the actual window state (double-click on the
    // drag region, keyboard, or the compositor all fire onResized).
    appWindow.isMaximized().then((m) => (isMaximized = m));
    const unResize = appWindow.onResized(() => {
      appWindow.isMaximized().then((m) => (isMaximized = m));
    });
    return () => unResize.then((f) => f());
  });
</script>

<header
  data-tauri-drag-region
  class="border-sidebar-border bg-sidebar relative flex h-11 shrink-0 items-center gap-2 border-b pr-2 pl-4"
>
  <!-- Amber signal hairline along the bottom edge of the titlebar. -->
  <span class="from-primary/50 pointer-events-none absolute inset-x-0 bottom-[-1px] h-px bg-gradient-to-r via-transparent to-transparent"></span>

  <div class="pointer-events-none flex items-baseline gap-2 select-none">
    <span class="bg-primary size-2 rotate-45 rounded-[2px]" aria-hidden="true"></span>
    <span class="font-display text-foreground text-[15px] font-semibold tracking-[0.22em]">DAYZLIN</span>
  </div>

  <div class="ml-auto flex items-center gap-0.5">
    <Button variant="ghost" size="icon-sm" aria-label="Minimize" onclick={() => appWindow.minimize()}>
      <Minus />
    </Button>
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={isMaximized ? "Restore" : "Maximize"}
      onclick={() => appWindow.toggleMaximize()}
    >
      {#if isMaximized}<Copy />{:else}<Square />{/if}
    </Button>
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label="Close"
      class="hover:bg-destructive hover:text-white"
      onclick={() => appWindow.close()}
    >
      <X />
    </Button>
  </div>
</header>
