<script lang="ts">
  // A thin capacity bar for server population. Fill width tracks value/max; colour signals how
  // joinable the server is — green with room, amber getting full, red effectively full.
  let { value = 0, max = 0, class: className = "" }: {
    value?: number;
    max?: number;
    class?: string;
  } = $props();

  const ratio = $derived(max > 0 ? Math.min(1, value / max) : 0);
  const color = $derived(
    max <= 0
      ? "bg-muted-foreground/40"
      : ratio >= 0.98
        ? "bg-destructive"
        : ratio >= 0.85
          ? "bg-warn"
          : "bg-online",
  );
</script>

<span class="bg-muted inline-block h-1.5 w-14 overflow-hidden rounded-full align-middle {className}">
  <span class="block h-full rounded-full transition-[width] {color}" style="width:{ratio * 100}%"></span>
</span>
