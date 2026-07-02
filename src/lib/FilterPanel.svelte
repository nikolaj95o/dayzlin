<script lang="ts">
  import type { ServerFilter } from "./api";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Toggle } from "$lib/components/ui/toggle/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as Combobox from "$lib/components/ui/combobox/index.js";
  import Search from "@lucide/svelte/icons/search";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Lock from "@lucide/svelte/icons/lock";
  import Users from "@lucide/svelte/icons/users";
  import UserCheck from "@lucide/svelte/icons/user-check";
  import Tag from "@lucide/svelte/icons/tag";

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
  const perspectiveLabels: Record<string, string> = { all: "All views", "1pp": "1PP", "3pp": "3PP" };

  function setPerspective(value: string) {
    filter.first_person_only = value === "1pp";
    filter.third_person_only = value === "3pp";
    onChange();
  }

  // Pressed toggle chip = amber signal; unpressed = quiet outline.
  const chip =
    "h-8 border border-input text-muted-foreground text-xs aria-pressed:bg-primary/15 aria-pressed:text-primary aria-pressed:border-primary/40";

  // Autocomplete: narrow the map list by what the user has typed.
  let mapSearch = $state("");
  let mapOpen = $state(false);
  const filteredMaps = $derived(
    mapOptions.filter((m) => m.toLowerCase().includes(mapSearch.toLowerCase())),
  );
</script>

<div class="flex flex-wrap items-center gap-2">
  <div class="relative">
    <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2" />
    <Input class="h-8 w-52 pl-8" placeholder="Search servers…" bind:value={query} oninput={onChange} />
  </div>

  <Combobox.Root
    type="single"
    bind:open={mapOpen}
    value={filter.map ?? ""}
    onValueChange={(v) => {
      filter.map = v || null;
      onChange();
    }}
    onOpenChange={(o) => {
      if (o) mapSearch = "";
    }}
  >
    <div class="relative">
      <Combobox.Input
        class="w-40 capitalize pr-8"
        placeholder="All maps"
        onclick={() => (mapOpen = true)}
        oninput={(e) => (mapSearch = e.currentTarget.value)}
      />
      <ChevronDown class="text-muted-foreground pointer-events-none absolute top-1/2 right-2 size-3.5 -translate-y-1/2" />
    </div>
    <Combobox.Content>
      <Combobox.Item value="" label="All maps">All maps</Combobox.Item>
      {#each filteredMaps as m}
        <Combobox.Item value={m} label={m}><span class="capitalize">{m}</span></Combobox.Item>
      {/each}
    </Combobox.Content>
  </Combobox.Root>

  <Select.Root type="single" value={perspective} onValueChange={setPerspective}>
    <Select.Trigger size="sm" class="w-28">{perspectiveLabels[perspective]}</Select.Trigger>
    <Select.Content>
      <Select.Item value="all" label="All views">All views</Select.Item>
      <Select.Item value="1pp" label="1PP">1PP</Select.Item>
      <Select.Item value="3pp" label="3PP">3PP</Select.Item>
    </Select.Content>
  </Select.Root>

  <Toggle
    size="sm"
    class={chip}
    bind:pressed={filter.no_password}
    onPressedChange={() => onChange()}
    aria-label="No password"
  >
    <Lock /> No password
  </Toggle>
  <Toggle
    size="sm"
    class={chip}
    bind:pressed={filter.has_slots}
    onPressedChange={() => onChange()}
    aria-label="Has slots"
  >
    <Users /> Has slots
  </Toggle>
  <Toggle
    size="sm"
    class={chip}
    bind:pressed={filter.not_empty}
    onPressedChange={() => onChange()}
    aria-label="Not empty"
    title="Hide servers with no players"
  >
    <UserCheck /> Not empty
  </Toggle>
  <Toggle
    size="sm"
    class={chip}
    bind:pressed={filter.same_version_only}
    onPressedChange={() => onChange()}
    aria-label="Same version"
    title="Hide servers whose game build differs from your installed DayZ"
  >
    <Tag /> Same version
  </Toggle>
</div>
