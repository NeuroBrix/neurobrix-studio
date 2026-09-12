import { beforeEach, expect, test, vi } from "vitest";
import { WindowTitle } from "./window-title.js";

const native = vi.hoisted(() => ({ isTauri: vi.fn(), setTitle: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: native.isTauri }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTitle: native.setTitle }),
}));

beforeEach(() => {
  vi.resetAllMocks();
  native.isTauri.mockReturnValue(true);
  native.setTitle.mockResolvedValue(undefined);
});

test("browser preview does not call a native window", async () => {
  native.isTauri.mockReturnValue(false);
  await new WindowTitle().update("Workspace");
  expect(native.setTitle).not.toHaveBeenCalled();
});

test("rapid navigation finishes with the latest title", async () => {
  const first = Promise.withResolvers<void>();
  native.setTitle.mockReturnValueOnce(first.promise);
  const title = new WindowTitle();
  const workspace = title.update("Workspace");
  const models = title.update("Models");
  const history = title.update("History");
  await vi.waitFor(() => expect(native.setTitle).toHaveBeenCalledExactlyOnceWith("Workspace"));
  first.resolve();
  await Promise.all([workspace, models, history]);
  expect(native.setTitle.mock.calls).toEqual([["Workspace"], ["Models"], ["History"]]);
});

test("reports failure to the caller and allows the next navigation", async () => {
  const error = new Error("Window unavailable");
  native.setTitle.mockRejectedValueOnce(error);
  const title = new WindowTitle();
  await expect(title.update("Models")).rejects.toBe(error);
  await expect(title.update("Settings")).resolves.toBeUndefined();
  expect(native.setTitle).toHaveBeenLastCalledWith("Settings");
});
