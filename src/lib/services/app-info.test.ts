import { beforeEach, expect, test, vi } from "vitest";
import { AppInfoError, getAppInfo } from "#lib/services/app-info.js";

const native = vi.hoisted(() => ({ isTauri: vi.fn(), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => native);

const appInfo = { version: "0.0.4", os: "macos", architecture: "aarch64" };

beforeEach(() => {
  vi.resetAllMocks();
  native.isTauri.mockReturnValue(true);
  native.invoke.mockResolvedValue(appInfo);
});

test("reads validated application information from the native command", async () => {
  await expect(getAppInfo()).resolves.toEqual(appInfo);
  expect(native.invoke).toHaveBeenCalledExactlyOnceWith("get_app_info");
});

test("ignores additional fields without treating them as validated capabilities", async () => {
  native.invoke.mockResolvedValue({ ...appInfo, gpuCompatible: true, engineVersion: "unknown" });
  await expect(getAppInfo()).resolves.toEqual(appInfo);
});

test.each([null, [], "untrusted response"])("rejects a non-object response: %j", async (value) => {
  native.invoke.mockResolvedValue(value);
  const failure = await getAppInfo().catch((error: unknown) => error);
  expect(failure).toBeInstanceOf(AppInfoError);
  expect(failure).toMatchObject({ code: "invalid-response" });
});

test.each(["version", "os", "architecture"] as const)(
  "rejects missing, blank or non-string %s",
  async (field) => {
    const withoutField = Object.fromEntries(
      Object.entries(appInfo).filter(([key]) => key !== field),
    );
    const invalidValues = [undefined, "", " \t\n", 42].map((value) => ({
      ...appInfo,
      [field]: value,
    }));
    for (const response of [withoutField, ...invalidValues]) {
      native.invoke.mockResolvedValue(response);
      const failure = await getAppInfo().catch((error: unknown) => error);
      expect(failure).toBeInstanceOf(AppInfoError);
      expect(failure).toMatchObject({ code: "invalid-response" });
    }
  },
);

test("browser preview reports unavailable without invoking native code", async () => {
  native.isTauri.mockReturnValue(false);
  const failure = await getAppInfo().catch((error: unknown) => error);
  expect(failure).toBeInstanceOf(AppInfoError);
  expect(failure).toMatchObject({ code: "unavailable" });
  expect(native.invoke).not.toHaveBeenCalled();
});

test.each([new Error("Private transport details"), "Private transport details"])(
  "keeps native failures out of the user message and allows a manual retry",
  async (cause) => {
    native.invoke.mockRejectedValueOnce(cause);
    const failure = await getAppInfo().catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(AppInfoError);
    expect(failure).toMatchObject({ code: "request-failed", cause });
    if (!(failure instanceof AppInfoError)) throw new Error("Expected a service error");
    expect(failure.message).not.toContain("Private transport details");
    expect(failure.message.trim()).not.toBe("");
    expect(native.invoke).toHaveBeenCalledTimes(1);

    await expect(getAppInfo()).resolves.toEqual(appInfo);
    expect(native.invoke).toHaveBeenCalledTimes(2);
  },
);
