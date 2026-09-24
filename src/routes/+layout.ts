import { engineDependencyKey, getEngineAvailability } from "#lib/services/engine-discovery.js";
import type { LayoutLoad } from "./$types";

// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

// Compile the automation bridge into explicit E2E builds only.
if (import.meta.env.MODE === "e2e" && typeof window !== "undefined") {
  await import("@wdio/tauri-plugin");
}

export const load: LayoutLoad = async ({ depends }) => {
  depends(engineDependencyKey);
  return { engine: await getEngineAvailability() };
};
