import { readFileSync } from "node:fs";
import { $, $$, browser, expect } from "@wdio/globals";
import { before, describe, it } from "mocha";

const { productName } = JSON.parse(
  readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
) as { productName: string };
const sections = ["Workspace", "Models", "History", "Settings"];
let appUrl: string;

async function expectPage(section: string) {
  await expect($("main h1")).toHaveText(section);
  await expect(browser).toHaveTitle(`${section} · ${productName}`);
  await expect($("nav[aria-label='Studio'] a[aria-current='page']")).toHaveText(section);
  await expect($$("nav a[aria-current='page']")).toBeElementsArrayOfSize(1);
  await browser.waitUntil(async () => (await nativeTitle()) === `${section} · ${productName}`);
}

async function nativeTitle() {
  return browser.execute(async () => {
    const api = (window as unknown as { __TAURI__: typeof import("@tauri-apps/api") }).__TAURI__;
    return api.window.getCurrentWindow().title();
  });
}

async function loadDocument(path?: string) {
  const url = path === undefined ? null : new URL(path, appUrl).href;
  await browser.execute(() => {
    document.documentElement.dataset.navigationPending = "true";
  });
  // Navigation must not destroy the document while execute() is collecting its result.
  if (url === null) await browser.refresh();
  else await browser.url(url);
  // Read a native snapshot while the old document is being replaced; execute()
  // stores its result in that document and can lose it during navigation.
  await browser.waitUntil(
    async () => !(await browser.getPageSource()).includes("data-navigation-pending="),
  );
  await expect($("main h1")).toBeDisplayed();
  await expect($$("nav[aria-label='Studio'] a")).toBeElementsArrayOfSize(4);
}

describe("navigation shell in the built desktop application", () => {
  before(async () => {
    // Keep the driver on this window while its document and native titles change.
    await browser.switchToWindow(await browser.getWindowHandle());
    // The embedded service may reuse the app between spec files.
    await $("nav a[href='/workspace']").click();
    await expectPage("Workspace");
    appUrl = await browser.getUrl();
  });

  it("keeps every route, selection and native title after direct entry and reload", async () => {
    for (const section of sections) {
      await loadDocument(`/${section.toLowerCase()}`);
      await expectPage(section);
      await loadDocument();
      await expectPage(section);
    }
  });

  it("keeps selection and titles in sync with back and forward navigation", async () => {
    await loadDocument("/workspace");
    await expectPage("Workspace");
    await $("nav a[href='/models']").click();
    await expectPage("Models");
    await $("nav a[href='/history']").click();
    await expectPage("History");
    await browser.back();
    await expectPage("Models");
    await browser.forward();
    await expectPage("History");
  });

  it("shows a real not-found page for unknown and obsolete nested routes", async () => {
    for (const path of ["/missing/page", "/workspace/models"]) {
      await loadDocument(path);
      await expect($("main h1")).toHaveText("Page not found");
      await expect(browser).toHaveTitle(`Page not found · ${productName}`);
      await browser.waitUntil(
        async () => (await nativeTitle()) === `Page not found · ${productName}`,
      );
      await expect($$("nav a[aria-current='page']")).toBeElementsArrayOfSize(0);
      await expect($("nav[aria-label='Studio']")).toBeDisplayed();
      await loadDocument();
      await expect($("main h1")).toHaveText("Page not found");
      await $("main a[href='/workspace']").click();
      await expectPage("Workspace");
    }
  });

  it("keeps links visible and separated at a narrow native window size", async () => {
    const original = await browser.getWindowSize();
    try {
      await browser.setWindowSize(320, 480);
      await browser.waitUntil(async () => (await browser.execute(() => innerWidth)) <= 320);
      for (const section of sections) {
        await $(`nav a[href='/${section.toLowerCase()}']`).click();
        await expectPage(section);
        const layout = await browser.execute(() => {
          const links = [...document.querySelectorAll("nav a")].map((link) => {
            const rect = link.getBoundingClientRect();
            return { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom };
          });
          const main = document.querySelector("main")?.getBoundingClientRect();
          return {
            width: innerWidth,
            scrollWidth: document.documentElement.scrollWidth,
            mainTop: main?.top,
            links,
          };
        });
        expect(layout.scrollWidth).toBeLessThanOrEqual(layout.width);
        for (const [index, link] of layout.links.entries()) {
          expect(link.left).toBeGreaterThanOrEqual(0);
          expect(link.right).toBeLessThanOrEqual(layout.width);
          expect(link.bottom).toBeLessThanOrEqual(layout.mainTop ?? 0);
          for (const other of layout.links.slice(index + 1)) {
            expect(
              link.right <= other.left ||
                other.right <= link.left ||
                link.bottom <= other.top ||
                other.bottom <= link.top,
            ).toBe(true);
          }
        }
      }
    } finally {
      await browser.setWindowSize(original.width, original.height);
    }
  });
});
