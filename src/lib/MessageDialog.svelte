<script lang="ts">
  import { dialog, closeDialog } from "./dialog";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";

  let showDetail = $state(false);

  // Reset the details toggle whenever a new message appears.
  $effect(() => {
    void $dialog;
    showDetail = false;
  });
</script>

<AlertDialog.Root open={$dialog !== null} onOpenChange={(o) => { if (!o) closeDialog(); }}>
  <AlertDialog.Content class="max-w-lg">
    {#if $dialog}
      <AlertDialog.Header>
        <AlertDialog.Title>{$dialog.title}</AlertDialog.Title>
        <AlertDialog.Description>{$dialog.message}</AlertDialog.Description>
      </AlertDialog.Header>
      {#if $dialog.detail}
        <button
          type="button"
          class="text-primary w-fit py-1 text-sm hover:underline"
          onclick={() => (showDetail = !showDetail)}
        >
          {showDetail ? "Hide" : "Show"} details
        </button>
        {#if showDetail}
          <pre class="bg-muted text-muted-foreground max-h-56 overflow-auto rounded-md border px-2.5 py-2 text-xs break-words whitespace-pre-wrap">{$dialog.detail}</pre>
        {/if}
      {/if}
      <AlertDialog.Footer>
        <AlertDialog.Action onclick={closeDialog}>Close</AlertDialog.Action>
      </AlertDialog.Footer>
    {/if}
  </AlertDialog.Content>
</AlertDialog.Root>
