<script lang="ts">
  import { launch, closeLaunch } from "./launch";
  import { cancelPlay } from "./api";

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

{#if $launch}
  <div class="fixed inset-0 z-[100] flex items-center justify-center">
    <div class="absolute inset-0 bg-black/50"></div>
    <div class="modal-card flex w-[calc(100%-3rem)] max-w-[420px] flex-col items-center gap-2.5 px-8 py-7 text-center" role="dialog" aria-modal="true" aria-busy="true" tabindex="-1">
      <div class="h-9 w-9 animate-spin rounded-full border-[3px] border-border border-t-accent motion-reduce:[animation-duration:2s]" aria-hidden="true"></div>

      {#if $launch.phase === "preparing"}
        <p class="m-0 text-[16px] text-text-h">Preparing…</p>
      {:else if $launch.phase === "downloading"}
        <p class="m-0 text-[16px] text-text-h">Downloading mods</p>
        <progress class="h-2 w-full" value={$launch.current} max={$launch.total}></progress>
        <p class="m-0 text-text [font-variant-numeric:tabular-nums]">{$launch.current} / {$launch.total}</p>
        <p class="m-0 text-[13px] text-text opacity-80 break-words">{$launch.name} ({$launch.id})</p>
        {#if $launch.total_bytes}
          <progress class="h-2 w-full" value={$launch.bytes} max={$launch.total_bytes}></progress>
        {/if}
        <p class="m-0 text-[13px] text-text opacity-70 [font-variant-numeric:tabular-nums]">{formatProgress($launch.bytes, $launch.total_bytes)}</p>
      {:else if $launch.phase === "launching"}
        <p class="m-0 text-[16px] text-text-h">Launching game…</p>
      {:else if $launch.phase === "starting"}
        <p class="m-0 text-[16px] text-text-h">DayZ is starting…</p>
      {/if}

      {#if canCancel}
        <button class="mt-2 cursor-pointer rounded-md border border-border bg-transparent px-3 py-1 font-[inherit] text-text transition-colors hover:border-accent-border hover:text-text-h" onclick={cancel}>Cancel</button>
      {/if}
    </div>
  </div>
{/if}
