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
  <div class="fixed inset-0 z-[100] flex items-center justify-center">
    <button class="absolute inset-0 cursor-default border-0 bg-black/50 p-0" aria-label="Close dialog" onclick={closeDialog}
    ></button>
    <div class="modal-card w-[calc(100%-3rem)] max-w-[520px] px-[22px] py-5" role="dialog" aria-modal="true" tabindex="-1">
      <h2 class="m-0 mb-2.5 text-[18px] text-text-h">{$dialog.title}</h2>
      <p class="leading-[1.45] text-text">{$dialog.message}</p>
      {#if $dialog.detail}
        <button class="cursor-pointer border-0 bg-none py-1.5 text-accent" onclick={() => (showDetail = !showDetail)}>
          {showDetail ? "Hide" : "Show"} details
        </button>
        {#if showDetail}
          <pre class="max-h-[220px] overflow-auto rounded-md border border-border bg-bg-alt px-2.5 py-2 text-xs break-words whitespace-pre-wrap">{$dialog.detail}</pre>
        {/if}
      {/if}
      <div class="mt-3.5 flex justify-end">
        <button class="btn" onclick={closeDialog}>Close</button>
      </div>
    </div>
  </div>
{/if}
