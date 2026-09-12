import { fileURLToPath } from "node:url";
import { type Browser, chromium, type Page } from "playwright";
import { createServer, type ViteDevServer } from "vite";
import { afterAll, beforeAll, expect, test } from "vitest";
import appConfig from "../../src-tauri/tauri.conf.json";

let server: ViteDevServer;
let browser: Browser;
let page: Page;
let baseUrl: string;

beforeAll(async () => {
  server = await createServer({
    configFile: fileURLToPath(new URL("../../vite.config.js", import.meta.url)),
    server: { port: 0, strictPort: false, open: false },
    logLevel: "error",
  });
  await server.listen();
  const url = server.resolvedUrls?.local[0];
  if (!url) throw new Error("Navigation test server did not expose a local URL.");
  baseUrl = url;
  browser = await chromium.launch();
  page = await browser.newPage();
});

afterAll(async () => {
  await browser?.close();
  await server?.close();
});

test("keyboard-only traversal reaches all four routes with visible focus and one active page", async () => {
  await page.goto(baseUrl);
  await expect.poll(() => page.locator("main h1").textContent()).toBe("Workspace");
  await page.keyboard.press("Tab");
  await expect.poll(() => page.locator(":focus").getAttribute("href")).toBe("#main-content");

  for (const name of ["Workspace", "Models", "History", "Settings"]) {
    const href = `/${name.toLowerCase()}`;
    await page.keyboard.press("Tab");
    await expect.poll(() => page.locator(":focus").getAttribute("href")).toBe(href);
    const outline = await page
      .locator(":focus")
      .evaluate((link) => getComputedStyle(link).outlineWidth);
    expect(Number.parseFloat(outline)).toBeGreaterThan(0);
    await page.keyboard.press("Enter");
    await expect.poll(() => page.locator("main h1").textContent()).toBe(name);
    await expect.poll(() => page.title()).toBe(`${name} · ${appConfig.productName}`);
    expect(await page.locator("nav a[aria-current='page']").count()).toBe(1);
    expect(await page.locator("nav a[aria-current='page']").textContent()).toBe(name);
    expect(await page.locator(":focus").getAttribute("href")).toBe(href);
  }

  await page.keyboard.press("Shift+Tab");
  await expect.poll(() => page.locator(":focus").getAttribute("href")).toBe("/history");
  await page.keyboard.press("Enter");
  await expect.poll(() => page.locator("main h1").textContent()).toBe("History");
});

test("the skip link moves keyboard focus to the page content", async () => {
  await page.goto(baseUrl);
  await expect.poll(() => page.locator("main h1").textContent()).toBe("Workspace");
  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await expect.poll(() => page.locator(":focus").getAttribute("id")).toBe("main-content");
});
