<script lang="ts">
  import EngineStatus from "#lib/components/engine-status.svelte";
  import { engineDependencyKey } from "#lib/services/engine-discovery.js";
  import { invalidate } from "$app/navigation";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();

  const refresh = () => invalidate(engineDependencyKey);
</script>

<h1 class="text-3xl font-semibold tracking-tight">Workspace</h1>
<EngineStatus availability={data.engine} {refresh} />
