<svelte:head>
  <title>NeuroBrix Studio</title>
</svelte:head>

<main class="mx-auto flex min-h-svh max-w-3xl flex-col justify-center gap-8 px-6 py-12">
  <header class="space-y-3">
    <p class="text-sm text-muted-foreground">Local AI workspace</p>
    <h1 class="text-3xl font-semibold tracking-tight">NeuroBrix Studio</h1>
    <p class="text-muted-foreground">A desktop interface for the NeuroBrix engine.</p>
  </header>

  <nav aria-label="Studio" class="grid gap-4 sm:grid-cols-2">
    <a
      href="/workspace/models"
      class="rounded-lg border bg-card p-6 transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring"
    >
      <h2 class="font-medium">Models</h2>
      <p class="mt-2 text-sm text-muted-foreground">Your local model library.</p>
    </a>
    <a
      href="/workspace/settings"
      class="rounded-lg border bg-card p-6 transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring"
    >
      <h2 class="font-medium">Settings</h2>
      <p class="mt-2 text-sm text-muted-foreground">Engine and application preferences.</p>
    </a>
  </nav>

  <p class="text-sm text-muted-foreground">
    In development. Engine integration and model management are not available yet.
  </p>
</main>
