<script lang="ts">
  import type { ServerFilter } from "./api";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as Select from "$lib/components/ui/select/index.js";

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
  const perspectiveLabels: Record<string, string> = { all: "All", "1pp": "1PP", "3pp": "3PP" };

  function setPerspective(value: string) {
    filter.first_person_only = value === "1pp";
    filter.third_person_only = value === "3pp";
    onChange();
  }
</script>

<div class="flex flex-wrap items-center gap-3 py-3">
  <Input class="w-56" placeholder="Search…" bind:value={query} oninput={onChange} />

  <Select.Root
    type="single"
    value={filter.map ?? ""}
    onValueChange={(v) => {
      filter.map = v || null;
      onChange();
    }}
  >
    <Select.Trigger class="w-40">{filter.map ?? "All maps"}</Select.Trigger>
    <Select.Content>
      <Select.Item value="" label="All maps">All maps</Select.Item>
      {#each mapOptions as m}
        <Select.Item value={m} label={m}>{m}</Select.Item>
      {/each}
    </Select.Content>
  </Select.Root>

  <Select.Root type="single" value={perspective} onValueChange={setPerspective}>
    <Select.Trigger class="w-28">{perspectiveLabels[perspective]}</Select.Trigger>
    <Select.Content>
      <Select.Item value="all" label="All">All</Select.Item>
      <Select.Item value="1pp" label="1PP">1PP</Select.Item>
      <Select.Item value="3pp" label="3PP">3PP</Select.Item>
    </Select.Content>
  </Select.Root>

  <Label class="gap-1.5">
    <Checkbox bind:checked={filter.no_password} onCheckedChange={() => onChange()} /> No password
  </Label>
  <Label class="gap-1.5">
    <Checkbox bind:checked={filter.has_slots} onCheckedChange={() => onChange()} /> Has slots
  </Label>
  <Label class="gap-1.5" title="Hide servers whose game build differs from your installed DayZ">
    <Checkbox bind:checked={filter.same_version_only} onCheckedChange={() => onChange()} /> Same version
  </Label>
</div>
