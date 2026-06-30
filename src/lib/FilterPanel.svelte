<script lang="ts">
  import type { ServerFilter } from "./api";

  let { filter = $bindable(), query = $bindable(), onChange }: {
    filter: ServerFilter;
    query: string;
    onChange: () => void;
  } = $props();
</script>

<div class="filters">
  <input placeholder="Search…" bind:value={query} oninput={onChange} />
  <input
    placeholder="Map"
    oninput={(e) => {
      filter.map = e.currentTarget.value || null;
      onChange();
    }}
  />
  <label>
    <input type="checkbox" bind:checked={filter.first_person_only} onchange={onChange} /> 1PP only
  </label>
  <label>
    <input type="checkbox" bind:checked={filter.no_password} onchange={onChange} /> No password
  </label>
  <label>
    <input type="checkbox" bind:checked={filter.has_slots} onchange={onChange} /> Has slots
  </label>
  <label title="Hide servers whose game build differs from your installed DayZ">
    <input type="checkbox" bind:checked={filter.same_version_only} onchange={onChange} /> Same version
  </label>
</div>
