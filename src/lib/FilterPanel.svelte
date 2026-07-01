<script lang="ts">
  import type { ServerFilter } from "./api";

  let { filter = $bindable(), query = $bindable(), onChange }: {
    filter: ServerFilter;
    query: string;
    onChange: () => void;
  } = $props();
</script>

<div class="flex flex-wrap items-center gap-3 py-3">
  <input class="field" placeholder="Search…" bind:value={query} oninput={onChange} />
  <input
    class="field"
    placeholder="Map"
    oninput={(e) => {
      filter.map = e.currentTarget.value || null;
      onChange();
    }}
  />
  <label class="inline-flex select-none items-center gap-1.5">
    <input type="checkbox" bind:checked={filter.first_person_only} onchange={onChange} /> 1PP
  </label>
  <label class="inline-flex select-none items-center gap-1.5">
    <input type="checkbox" bind:checked={filter.third_person_only} onchange={onChange} /> 3PP
  </label>
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
