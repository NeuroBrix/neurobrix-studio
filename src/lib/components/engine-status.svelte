<script lang="ts">
  import type { EngineAvailability } from "#lib/services/engine-discovery.js";

  interface Props {
    availability: EngineAvailability;
    refresh: () => void;
  }

  let { availability, refresh }: Props = $props();
</script>

<!-- Placeholder until the workspace design is defined; the five states must stay distinguishable. -->
{#if availability.state === "compatible"}
  <p>NeuroBrix engine ready — version {availability.engine_version}.</p>
{:else if availability.state === "incompatible"}
  <p>
    Engine {availability.engine_version} is outside the tested range ({availability.supported_range}).
  </p>
{:else if availability.state === "absent"}
  <p>No NeuroBrix engine found on this computer.</p>
{:else if availability.state === "unavailable"}
  <p>Engine status unavailable: {availability.reason}</p>
  <button type="button" onclick={refresh}>Check again</button>
{:else}
  <p>Engine sent an unrecognized status: {availability.reason}</p>
  <button type="button" onclick={refresh}>Check again</button>
{/if}
