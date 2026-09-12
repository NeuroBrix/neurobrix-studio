import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    projects: [
      {
        resolve: { alias: { "#lib": fileURLToPath(new URL("./src/lib", import.meta.url)) } },
        test: {
          name: "unit",
          environment: "node",
          include: ["src/**/*.test.ts"],
          exclude: ["src/**/*.svelte.test.ts"],
        },
      },
      {
        plugins: [svelte({ configFile: false })],
        resolve: { alias: { "#lib": fileURLToPath(new URL("./src/lib", import.meta.url)) } },
        test: {
          name: "component",
          include: ["src/**/*.svelte.test.ts"],
          setupFiles: ["vitest-browser-svelte"],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            instances: [{ browser: "chromium" }],
          },
        },
      },
    ],
  },
});
