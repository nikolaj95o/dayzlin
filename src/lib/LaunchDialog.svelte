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
  <div class="overlay">
    <div class="backdrop"></div>
    <div class="modal" role="dialog" aria-modal="true" aria-busy="true" tabindex="-1">
      <div class="spinner" aria-hidden="true"></div>

      {#if $launch.phase === "preparing"}
        <p class="phase">Preparing…</p>
      {:else if $launch.phase === "downloading"}
        <p class="phase">Downloading mods</p>
        <progress value={$launch.current} max={$launch.total}></progress>
        <p class="count">{$launch.current} / {$launch.total}</p>
        <p class="mod">{$launch.name} ({$launch.id})</p>
        {#if $launch.total_bytes}
          <progress value={$launch.bytes} max={$launch.total_bytes}></progress>
        {/if}
        <p class="size">{formatProgress($launch.bytes, $launch.total_bytes)}</p>
      {:else if $launch.phase === "launching"}
        <p class="phase">Launching game…</p>
      {:else if $launch.phase === "starting"}
        <p class="phase">DayZ is starting…</p>
      {/if}

      {#if canCancel}
        <button class="cancel" onclick={cancel}>Cancel</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
  }
  .modal {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 28px 32px;
    max-width: 420px;
    width: calc(100% - 48px);
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.35);
    text-align: center;
  }
  .spinner {
    width: 36px;
    height: 36px;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .phase {
    margin: 0;
    font-size: 16px;
    color: var(--text-h);
  }
  progress {
    width: 100%;
    height: 8px;
  }
  .count {
    margin: 0;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .mod {
    margin: 0;
    font-size: 13px;
    color: var(--text);
    opacity: 0.8;
    word-break: break-word;
  }
  .size {
    margin: 0;
    font-size: 13px;
    color: var(--text);
    opacity: 0.7;
    font-variant-numeric: tabular-nums;
  }
  .cancel {
    margin-top: 8px;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text);
  }
  .cancel:hover {
    border-color: var(--accent-border);
    color: var(--text-h);
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2s;
    }
  }
</style>
