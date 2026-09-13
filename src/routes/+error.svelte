<script lang="ts">
  import { Button } from "#lib/components/ui/button/index.js";
  import { resolve } from "$app/paths";
  import { page } from "$app/state";
</script>

<h1 class="text-3xl font-semibold tracking-tight">
  {page.status === 404 ? "Page not found" : "Unable to open page"}
</h1>
<p class="max-w-prose text-muted-foreground">
  {page.status === 404
    ? "This page does not exist. Choose a section from the navigation or return to Workspace."
    : "This page could not be opened. Return to Workspace and try again."}
</p>
<Button href={resolve("/workspace")}>Return to Workspace</Button>
