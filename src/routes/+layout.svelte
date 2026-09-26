<script lang="ts">
  import "./layout.css";
  import type { Snippet } from "svelte";
  import { appName, destinations } from "#lib/navigation.js";
  import { WindowTitle } from "#lib/services/window-title.js";
  import { resolve } from "$app/paths";
  import { page } from "$app/state";

  let { children }: { children: Snippet } = $props();
  const windowTitle = new WindowTitle();
  let titleError = $state(false);
  const activePage = $derived(
    page.error ? undefined : destinations.find((item) => item.href === page.route.id),
  );
  const pageName = $derived(
    page.status === 404 ? "Page not found" : page.error ? "Unable to open page" : activePage?.label,
  );
  const title = $derived(pageName ? `${pageName} · ${appName}` : appName);

  $effect(() => {
    let current = true;
    titleError = false;
    windowTitle.update(title).catch((error: unknown) => {
      console.error("Could not update the native window title", error);
      if (current) titleError = true;
    });
    return () => {
      current = false;
    };
  });
</script>

<svelte:head>
  <title>{title}</title>
</svelte:head>

<a
  href="#main-content"
  class="sr-only z-10 rounded-md bg-background p-3 text-foreground focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:outline-2 focus:outline-offset-2 focus:outline-ring"
  >Skip to content</a
>

<div class="grid min-h-svh md:grid-cols-[14rem_minmax(0,1fr)]">
  <header class="min-w-0 border-b bg-sidebar p-4 text-sidebar-foreground md:border-r md:border-b-0">
    <p class="mb-5 break-words text-lg font-semibold">{appName}</p>
    <nav aria-label="Studio" data-sveltekit-reset="false">
      <ul class="grid grid-cols-2 gap-2 md:grid-cols-1">
        {#each destinations as destination (destination.href)}
          <li class="min-w-0">
            <a
              href={resolve(destination.href)}
              aria-current={activePage === destination ? "page" : undefined}
              class="block rounded-md border-2 border-transparent px-3 py-2 break-words hover:bg-sidebar-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring aria-[current=page]:border-sidebar-primary aria-[current=page]:bg-sidebar-accent aria-[current=page]:font-semibold"
              >{destination.label}</a
            >
          </li>
        {/each}
      </ul>
    </nav>
  </header>

  <main id="main-content" tabindex="-1" class="min-w-0 space-y-6 p-6 md:p-10">
    {#if titleError}
      <p role="alert" class="rounded-md border border-destructive p-3 text-sm">
        The window title could not be updated. Reopen Studio to try again.
      </p>
    {/if}
    {@render children()}
    <p class="max-w-prose text-sm text-muted-foreground">
      In development. Models and inference are not available yet.
    </p>
  </main>
</div>
