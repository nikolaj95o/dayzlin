<script lang="ts">
  import { dialog, closeDialog } from "./dialog";

  let showDetail = $state(false);

  // Reset the details toggle whenever a new message appears.
  $effect(() => {
    void $dialog;
    showDetail = false;
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && closeDialog()} />

{#if $dialog}
  <div class="overlay">
    <button class="backdrop" aria-label="Close dialog" onclick={closeDialog}
    ></button>
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <h2>{$dialog.title}</h2>
      <p class="msg">{$dialog.message}</p>
      {#if $dialog.detail}
        <button class="link" onclick={() => (showDetail = !showDetail)}>
          {showDetail ? "Hide" : "Show"} details
        </button>
        {#if showDetail}
          <pre class="detail">{$dialog.detail}</pre>
        {/if}
      {/if}
      <div class="actions">
        <button onclick={closeDialog}>Close</button>
      </div>
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
    border: none;
    padding: 0;
    background: rgba(0, 0, 0, 0.5);
    cursor: default;
  }
  .modal {
    position: relative;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 20px 22px;
    max-width: 520px;
    width: calc(100% - 48px);
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.35);
  }
  .modal h2 {
    margin: 0 0 10px;
    font-size: 18px;
    color: var(--text-h);
  }
  .msg {
    color: var(--text);
    line-height: 1.45;
  }
  .link {
    background: none;
    border: none;
    color: var(--accent);
    padding: 6px 0;
    cursor: pointer;
  }
  .detail {
    max-height: 220px;
    overflow: auto;
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 14px;
  }
</style>
