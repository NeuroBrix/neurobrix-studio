<script lang="ts">
  import { onMount } from "svelte";
  import { Button } from "#lib/components/ui/button/index.js";
  import { AppInfoResource } from "#lib/states/app-info.svelte.js";

  const info = new AppInfoResource();

  onMount(() => {
    void info.load();
    return () => info.dispose();
  });
</script>

<section aria-labelledby="app-information-heading" class="max-w-xl space-y-5 rounded-lg border p-5">
  <div class="space-y-1">
    <h2 id="app-information-heading" class="text-lg font-semibold">About this app</h2>
    <p class="text-sm text-muted-foreground">Details reported by the desktop application.</p>
  </div>

  <div role="status" aria-live="polite" class="text-sm text-muted-foreground">
    {#if info.isLoading}
      {info.data ? "Refreshing app information…" : "Loading app information…"}
    {:else if info.data && !info.error}
      App information loaded.
    {/if}
  </div>

  {#if info.data}
    <dl class="space-y-3 text-sm">
      <div class="grid gap-1 sm:grid-cols-[10rem_minmax(0,1fr)]">
        <dt class="text-muted-foreground">Version</dt>
        <dd class="break-words font-medium" data-testid="app-version">{info.data.version}</dd>
      </div>
      <div class="grid gap-1 sm:grid-cols-[10rem_minmax(0,1fr)]">
        <dt class="text-muted-foreground">Operating system</dt>
        <dd class="break-words font-medium" data-testid="app-os">{info.data.os}</dd>
      </div>
      <div class="grid gap-1 sm:grid-cols-[10rem_minmax(0,1fr)]">
        <dt class="text-muted-foreground">App architecture</dt>
        <dd class="break-words font-medium" data-testid="app-architecture">
          {info.data.architecture}
        </dd>
      </div>
    </dl>
  {/if}

  {#if info.error}
    <div role="alert" class="space-y-2 rounded-md border p-3 text-sm">
      <p>{info.error.message}</p>
      {#if info.data}
        <p class="text-muted-foreground">Showing the last successful result.</p>
      {/if}
    </div>
  {/if}

  {#if info.error?.code !== "unavailable"}
    <Button variant="outline" disabled={info.isLoading} onclick={() => info.load()}>
      {info.error ? "Try again" : "Refresh"}
    </Button>
  {/if}
</section>
