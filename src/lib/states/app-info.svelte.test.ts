import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";
import { type AppInfo, AppInfoError } from "#lib/services/app-info.js";
import { AppInfoResource } from "#lib/states/app-info.svelte.js";

const appInfo: AppInfo = { version: "0.0.4", os: "macos", architecture: "aarch64" };
const refreshedInfo: AppInfo = { ...appInfo, version: "0.0.5" };

test("publishes initial loading and successful data through reactive state", async () => {
  const request = Promise.withResolvers<AppInfo>();
  const read = vi.fn(() => request.promise);
  const resource = new AppInfoResource(read);
  const observed: { data: AppInfo | undefined; isLoading: boolean; error: AppInfoError | null }[] =
    [];
  const stop = $effect.root(() => {
    $effect(() => {
      observed.push({ data: resource.data, isLoading: resource.isLoading, error: resource.error });
    });
  });

  try {
    flushSync();
    expect(observed.at(-1)).toEqual({ data: undefined, isLoading: false, error: null });
    expect(read).not.toHaveBeenCalled();

    const loading = resource.load();
    flushSync();
    expect(observed.at(-1)).toEqual({ data: undefined, isLoading: true, error: null });

    request.resolve(appInfo);
    await loading;
    flushSync();
    expect(observed.at(-1)).toEqual({ data: appInfo, isLoading: false, error: null });
    expect(read).toHaveBeenCalledOnce();
  } finally {
    stop();
    resource.dispose();
  }
});

test("exposes an initial failure until an explicit retry succeeds", async () => {
  const retry = Promise.withResolvers<AppInfo>();
  const failure = new AppInfoError("unavailable");
  const read = vi
    .fn<() => Promise<AppInfo>>()
    .mockRejectedValueOnce(failure)
    .mockReturnValueOnce(retry.promise);
  const resource = new AppInfoResource(read);

  await resource.load();
  flushSync();
  expect(resource.data).toBeUndefined();
  expect(resource.error).toBe(failure);
  expect(resource.isLoading).toBe(false);
  expect(read).toHaveBeenCalledOnce();

  const loading = resource.load();
  expect(resource.error).toBeNull();
  expect(resource.data).toBeUndefined();
  expect(resource.isLoading).toBe(true);
  retry.resolve(appInfo);
  await loading;
  expect(resource.data).toEqual(appInfo);
  expect(resource.error).toBeNull();
  expect(resource.isLoading).toBe(false);
  expect(read).toHaveBeenCalledTimes(2);
});

test("keeps the last success during refresh and alongside a refresh failure", async () => {
  const refresh = Promise.withResolvers<AppInfo>();
  const failure = new AppInfoError("request-failed");
  const read = vi
    .fn<() => Promise<AppInfo>>()
    .mockResolvedValueOnce(appInfo)
    .mockReturnValueOnce(refresh.promise)
    .mockResolvedValueOnce(refreshedInfo);
  const resource = new AppInfoResource(read);
  await resource.load();

  const loading = resource.load();
  expect(resource.data).toEqual(appInfo);
  expect(resource.isLoading).toBe(true);
  refresh.reject(failure);
  await loading;
  expect(resource.data).toEqual(appInfo);
  expect(resource.error).toBe(failure);
  expect(resource.isLoading).toBe(false);
  expect(read).toHaveBeenCalledTimes(2);

  await resource.load();
  expect(resource.data).toEqual(refreshedInfo);
  expect(resource.error).toBeNull();
});

test.each(["success", "failure"] as const)(
  "ignores an older %s while the latest request is pending",
  async (outcome) => {
    const older = Promise.withResolvers<AppInfo>();
    const latest = Promise.withResolvers<AppInfo>();
    const read = vi
      .fn<() => Promise<AppInfo>>()
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(latest.promise);
    const resource = new AppInfoResource(read);
    const firstLoad = resource.load();
    const latestLoad = resource.load();

    if (outcome === "success") older.resolve(appInfo);
    else older.reject(new AppInfoError("invalid-response"));
    await firstLoad;
    expect(resource.data).toBeUndefined();
    expect(resource.error).toBeNull();
    expect(resource.isLoading).toBe(true);

    latest.resolve(refreshedInfo);
    await latestLoad;
    expect(resource.data).toEqual(refreshedInfo);
    expect(resource.error).toBeNull();
    expect(resource.isLoading).toBe(false);
  },
);

test.each(["success", "failure"] as const)(
  "preserves the latest failure when an older request completes with %s",
  async (outcome) => {
    const older = Promise.withResolvers<AppInfo>();
    const latest = Promise.withResolvers<AppInfo>();
    const failure = new AppInfoError("request-failed");
    const read = vi
      .fn<() => Promise<AppInfo>>()
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(latest.promise);
    const resource = new AppInfoResource(read);
    const firstLoad = resource.load();
    const latestLoad = resource.load();

    latest.reject(failure);
    await latestLoad;
    if (outcome === "success") older.resolve(appInfo);
    else older.reject(new AppInfoError("invalid-response"));
    await firstLoad;
    expect(resource.data).toBeUndefined();
    expect(resource.error).toBe(failure);
    expect(resource.isLoading).toBe(false);
  },
);

test.each(["success", "failure"] as const)(
  "ignores %s after disposal and never starts another request",
  async (outcome) => {
    const request = Promise.withResolvers<AppInfo>();
    const read = vi.fn(() => request.promise);
    const resource = new AppInfoResource(read);
    const loading = resource.load();
    resource.dispose();
    expect(resource.isLoading).toBe(false);

    if (outcome === "success") request.resolve(appInfo);
    else request.reject(new AppInfoError("request-failed"));
    await loading;
    await resource.load();
    expect(resource.data).toBeUndefined();
    expect(resource.error).toBeNull();
    expect(resource.isLoading).toBe(false);
    expect(read).toHaveBeenCalledOnce();
  },
);

test("turns an unexpected reader rejection into a safe actionable error", async () => {
  const cause = { detail: "Private reader details" };
  const resource = new AppInfoResource(vi.fn().mockRejectedValue(cause));
  await resource.load();
  expect(resource.error).toBeInstanceOf(AppInfoError);
  expect(resource.error).toMatchObject({ code: "request-failed", cause });
  expect(resource.error?.message).not.toContain("Private reader details");
  expect(resource.data).toBeUndefined();
  expect(resource.isLoading).toBe(false);
});
