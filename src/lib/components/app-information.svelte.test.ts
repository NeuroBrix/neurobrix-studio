import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { userEvent } from "vitest/browser";
import { render } from "vitest-browser-svelte";
import type { AppInfo } from "#lib/services/app-info.js";
import AppInformation from "./app-information.svelte";

const fixture: AppInfo = {
  version: "1.2.3-test.1",
  os: "fixture-os",
  architecture: "fixture-arch",
};
const read = vi.fn<() => Promise<unknown>>();

beforeEach(() => {
  read.mockReset();
  vi.stubGlobal("isTauri", true);
  mockIPC((command) => {
    if (command !== "get_app_info") throw new Error(`Unexpected native command: ${command}`);
    return read();
  });
});

afterEach(() => {
  clearMocks();
  vi.unstubAllGlobals();
});

test("announces loading then displays the returned app details after a single read", async () => {
  const pending = Promise.withResolvers<AppInfo>();
  read.mockReturnValueOnce(pending.promise);
  const screen = await render(AppInformation);
  await expect.element(screen.getByRole("status")).toHaveTextContent("Loading app information…");
  await expect.element(screen.getByRole("button", { name: "Refresh" })).toBeDisabled();
  pending.resolve(fixture);
  await expect.element(screen.getByText(fixture.version, { exact: true })).toBeVisible();
  await expect.element(screen.getByText(fixture.os, { exact: true })).toBeVisible();
  await expect.element(screen.getByText(fixture.architecture, { exact: true })).toBeVisible();
  await expect.element(screen.getByRole("status")).toHaveTextContent("App information loaded.");
  expect(read).toHaveBeenCalledOnce();
});

test("reports failure and supports an explicit keyboard retry", async () => {
  const pending = Promise.withResolvers<AppInfo>();
  read.mockRejectedValueOnce(new Error("Test IPC failure")).mockReturnValueOnce(pending.promise);
  const screen = await render(AppInformation);
  await expect
    .element(screen.getByRole("alert"))
    .toHaveTextContent("Studio could not read its app information. Try again or reopen the app.");
  const retry = screen.getByRole("button", { name: "Try again" });
  await userEvent.tab();
  await expect.element(retry).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  await expect.element(screen.getByRole("button", { name: "Refresh" })).toBeDisabled();
  expect(read).toHaveBeenCalledTimes(2);
  pending.resolve(fixture);
  await expect.element(screen.getByText(fixture.version, { exact: true })).toBeVisible();
  await expect.element(screen.getByRole("alert")).not.toBeInTheDocument();
});

test("keeps the last result during a failed refresh, then replaces it on retry", async () => {
  const refresh = Promise.withResolvers<unknown>();
  const next = { ...fixture, version: "1.2.4-test.1" };
  read
    .mockResolvedValueOnce(fixture)
    .mockReturnValueOnce(refresh.promise)
    .mockResolvedValueOnce(next);
  const screen = await render(AppInformation);
  await expect.element(screen.getByText(fixture.version, { exact: true })).toBeVisible();
  await screen.getByRole("button", { name: "Refresh" }).click();
  await expect.element(screen.getByRole("status")).toHaveTextContent("Refreshing app information…");
  await expect.element(screen.getByText(fixture.version, { exact: true })).toBeVisible();
  refresh.resolve({ version: null });
  await expect
    .element(screen.getByText("Showing the last successful result.", { exact: true }))
    .toBeVisible();
  await screen.getByRole("button", { name: "Try again" }).click();
  await expect.element(screen.getByText(next.version, { exact: true })).toBeVisible();
  await expect.element(screen.getByText(fixture.version, { exact: true })).not.toBeInTheDocument();
  await expect.element(screen.getByRole("alert")).not.toBeInTheDocument();
});

test("explains browser unavailability without fake details or a pointless retry", async () => {
  vi.stubGlobal("isTauri", false);
  const screen = await render(AppInformation);
  await expect
    .element(screen.getByRole("alert"))
    .toHaveTextContent(
      "App information is available in the desktop app. Open NeuroBrix Studio to view it.",
    );
  await expect.element(screen.getByRole("button")).not.toBeInTheDocument();
  await expect.element(screen.getByText("Version", { exact: true })).not.toBeInTheDocument();
  expect(read).not.toHaveBeenCalled();
});

test("leaving and reopening the panel owns a fresh read and discards the old result", async () => {
  const old = Promise.withResolvers<AppInfo>();
  const current = Promise.withResolvers<AppInfo>();
  read.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
  const first = await render(AppInformation);
  await expect.element(first.getByRole("status")).toHaveTextContent("Loading app information…");
  await first.unmount();
  const second = await render(AppInformation);
  current.resolve(fixture);
  await expect.element(second.getByText(fixture.version, { exact: true })).toBeVisible();
  old.resolve({ ...fixture, version: "0.0.1-old" });
  await old.promise;
  await expect.element(second.getByText(fixture.version, { exact: true })).toBeVisible();
  await expect.element(second.getByText("0.0.1-old", { exact: true })).not.toBeInTheDocument();
  expect(read).toHaveBeenCalledTimes(2);
});
