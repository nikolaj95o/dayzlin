<script lang="ts">
  import type { Server } from "./api";

  let { servers = [], onSelect }: {
    servers: Server[];
    onSelect: (s: Server) => void;
  } = $props();
</script>

<table>
  <thead>
    <tr>
      <th>Name</th>
      <th>Map</th>
      <th>Players</th>
      <th>Mods</th>
      <th>View</th>
      <th></th>
    </tr>
  </thead>
  <tbody>
    {#each servers as s}
      <tr>
        <td class="name">{s.name}</td>
        <td>{s.map}</td>
        <td>{s.players}/{s.max_players}</td>
        <td>{s.mods.length}</td>
        <td>{s.first_person ? "1PP" : "3PP"}</td>
        <td><button onclick={() => onSelect(s)}>Play</button></td>
      </tr>
    {:else}
      <tr><td colspan="6" class="empty">No servers</td></tr>
    {/each}
  </tbody>
</table>
