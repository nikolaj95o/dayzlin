<script lang="ts">
  import type { ServerFilter } from "./api";

  let { filter = $bindable(), query = $bindable(), mapOptions, onChange }: {
    filter: ServerFilter;
    query: string;
    mapOptions: string[];
    onChange: () => void;
  } = $props();

  // Collapse the two perspective booleans into a single select value.
  const perspective = $derived(
    filter.first_person_only ? "1pp" : filter.third_person_only ? "3pp" : "all",
  );
  function setPerspective(value: string) {
    filter.first_person_only = value === "1pp";
    filter.third_person_only = value === "3pp";
    onChange();
  }
</script>

<div class="flex flex-wrap items-center gap-3 py-3">
  <input class="field" placeholder="Search…" bind:value={query} oninput={onChange} />
  <select
    class="field"
    value={filter.map ?? ""}
    onchange={(e) => {
      filter.map = e.currentTarget.value || null;
      onChange();
    }}
  >
    <option value="">All maps</option>
    {#each mapOptions as m}
      <option value={m}>{m}</option>
    {/each}
  </select>
  <select class="field" value={perspective} onchange={(e) => setPerspective(e.currentTarget.value)}>
    <option value="all">All</option>
    <option value="1pp">1PP</option>
    <option value="3pp">3PP</option>
  </select>
  <label class="inline-flex select-none items-center gap-1.5">
    <input type="checkbox" bind:checked={filter.no_password} onchange={onChange} /> No password
  </label>
  <label class="inline-flex select-none items-center gap-1.5">
    <input type="checkbox" bind:checked={filter.has_slots} onchange={onChange} /> Has slots
  </label>
  <label class="inline-flex select-none items-center gap-1.5" title="Hide servers whose game build differs from your installed DayZ">
    <input type="checkbox" bind:checked={filter.same_version_only} onchange={onChange} /> Same version
  </label>
</div>
