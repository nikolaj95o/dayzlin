<script lang="ts">
  import { launch, closeLaunch } from "./launch";
  import { cancelPlay } from "./api";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Progress } from "$lib/components/ui/progress/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";

  // Cancel is only meaningful while we're still preparing/downloading; once Steam has been
  // handed off ("launching"/"starting") there's nothing left to abort.
  let canCancel = $derived(
    $launch?.phase === "preparing" || $launch?.phase === "downloading",
  );

  // Format a byte count as MB/GB (no "starting…" fallback; that's handled by the caller).
  function formatMb(b: number): string {
    const mb = b / (1024 * 1024);
    return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(1)} MB`;
  }

  // Live progress line for the mod currently downloading. When Steam reported a total
  // size we show "X MB of Y MB"; otherwise just the bytes downloaded so far.
  function formatProgress(bytes: number, total: number | null): string {
    if (!bytes) return "starting…";
    return total ? `${formatMb(bytes)} of ${formatMb(total)}` : formatMb(bytes);
  }

  async function cancel() {
    closeLaunch(); // unblock the UI immediately
    try {
      await cancelPlay();
    } catch {
      // best-effort; the launch command will return a cancelled error regardless
    }
  }
</script>

<!-- Non-dismissable while a launch is in flight: no close button, and outside-click / Escape
     are ignored. It closes only when the launch store is cleared. -->
<Dialog.Root open={$launch !== null}>
  <Dialog.Content
    showCloseButton={false}
    interactOutsideBehavior="ignore"
    escapeKeydownBehavior="ignore"
    class="max-w-md items-center text-center"
  >
    {#if $launch}
      <Dialog.Header class="items-center">
        <LoaderCircle
          class="text-primary size-9 animate-spin motion-reduce:[animation-duration:2s]"
          aria-hidden="true"
        />
        <Dialog.Title>
          {#if $launch.phase === "preparing"}Preparing…
          {:else if $launch.phase === "downloading"}Downloading mods
          {:else if $launch.phase === "launching"}Launching game…
          {:else if $launch.phase === "starting"}DayZ is starting…{/if}
        </Dialog.Title>
        <Dialog.Description class="sr-only">Launching DayZ</Dialog.Description>
      </Dialog.Header>

      {#if $launch.phase === "downloading"}
        <div class="flex w-full flex-col items-center gap-2">
          <Progress value={$launch.current} max={$launch.total} />
          <p class="text-muted-foreground m-0 text-sm tabular-nums">{$launch.current} / {$launch.total}</p>
          <p class="text-muted-foreground m-0 text-sm break-words opacity-80">{$launch.name} ({$launch.id})</p>
          {#if $launch.total_bytes}
            <Progress value={$launch.bytes} max={$launch.total_bytes} />
          {/if}
          <p class="text-muted-foreground m-0 text-xs tabular-nums opacity-70">{formatProgress($launch.bytes, $launch.total_bytes)}</p>
        </div>
      {/if}

      {#if canCancel}
        <Button variant="outline" onclick={cancel}>Cancel</Button>
      {/if}
    {/if}
  </Dialog.Content>
</Dialog.Root>
