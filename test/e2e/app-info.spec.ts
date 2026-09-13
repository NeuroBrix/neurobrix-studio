import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { $, browser, expect } from "@wdio/globals";
import { before, describe, it } from "mocha";

const root = fileURLToPath(new URL("../../", import.meta.url));
const { version } = JSON.parse(
  readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
) as { version: string };
const target = execFileSync("rustc", ["--print", "cfg"], { cwd: root, encoding: "utf8" });

function targetValue(key: string): string {
  const match = target.match(new RegExp(`^${key}="([^"]+)"$`, "m"));
  if (!match) throw new Error(`Rust did not report ${key} for the native test build.`);
  return match[1];
}

const os = targetValue("target_os");
const architecture = targetValue("target_arch");

async function expectAppInformation() {
  await expect($("main h1")).toHaveText("Settings");
  await expect($("[data-testid='app-version']")).toHaveText(version);
  await expect($("[data-testid='app-os']")).toHaveText(os);
  await expect($("[data-testid='app-architecture']")).toHaveText(architecture);
  await expect($("section[aria-labelledby='app-information-heading'] [role='status']")).toHaveText(
    "App information loaded.",
  );
  await expect(
    $("section[aria-labelledby='app-information-heading'] [role='alert']"),
  ).not.toExist();
}

describe("app information through the built desktop interface and real IPC", () => {
  before(async () => {
    await browser.switchToWindow(await browser.getWindowHandle());
    await expect($("nav[aria-label='Studio']")).toBeDisplayed();
  });

  it("shows the configured app version and compiled platform in Settings and refreshes them", async () => {
    await $("nav a[href='/settings']").click();
    await expectAppInformation();
    await $("button=Refresh").click();
    await expectAppInformation();
  });

  it("reads app information again after leaving Settings and reloading its document", async () => {
    await $("nav a[href='/workspace']").click();
    await expect($("main h1")).toHaveText("Workspace");
    await $("nav a[href='/settings']").click();
    await expectAppInformation();
    await browser.execute(() => {
      document.documentElement.dataset.appInfoReloadPending = "true";
    });
    await browser.refresh();
    // The snapshot survives document replacement; execute() keeps its result in
    // the old page and can lose it while a reload is completing.
    await browser.waitUntil(
      async () => !(await browser.getPageSource()).includes("data-app-info-reload-pending="),
    );
    await expectAppInformation();
  });
});
