import * as v from "valibot";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { engineAvailabilitySchema, getEngineAvailability } from "./engine-discovery.js";

const native = vi.hoisted(() => ({ isTauri: vi.fn(), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: native.isTauri,
  invoke: native.invoke,
}));

const valid = (state: string, extra: Record<string, unknown> = {}) => ({
  state,
  ...extra,
});

describe("engineAvailabilitySchema", () => {
  test.each([
    [
      "compatible",
      valid("compatible", {
        engine_version: "0.6.0",
        protocol_version: "1",
        capabilities: ["chat"],
      }),
    ],
    ["compatible without optional fields", valid("compatible", { engine_version: "0.6.0" })],
    [
      "incompatible",
      valid("incompatible", {
        engine_version: "0.7.0",
        supported_range: ">=0.5.3,<0.6",
      }),
    ],
    ["absent", valid("absent")],
    ["unavailable", valid("unavailable", { reason: "timed out" })],
    ["contract_failure", valid("contract_failure", { reason: "bad json" })],
  ])("accepts the documented %s response", (_name, input) => {
    expect(v.parse(engineAvailabilitySchema, input)).toBeTruthy();
  });

  test.each([
    ["missing state", {}],
    ["unknown state", valid("compatible!", {})],
    ["wrong field type", valid("compatible", { engine_version: 0.6 })],
    ["incompatible without range", valid("incompatible", { engine_version: "0.7.0" })],
  ])("rejects malformed response: %s", (_name, input) => {
    const parsed = v.safeParse(engineAvailabilitySchema, input);
    expect(parsed.success).toBe(false);
  });
});

describe("getEngineAvailability", () => {
  beforeEach(() => vi.clearAllMocks());

  test("validates the Tauri response and returns it", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockResolvedValue(valid("absent"));
    expect(await getEngineAvailability()).toEqual({ state: "absent" });
    expect(native.invoke).toHaveBeenCalledWith("engine_discovery");
  });

  test("maps an unrecognizable Tauri response to contract_failure", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockResolvedValue({ state: "compatible" }); // missing engine_version
    expect(await getEngineAvailability()).toMatchObject({
      state: "contract_failure",
    });
  });

  test("reports browser preview explicitly instead of faking data", async () => {
    native.isTauri.mockReturnValue(false);
    expect(await getEngineAvailability()).toMatchObject({
      state: "unavailable",
    });
    expect(native.invoke).not.toHaveBeenCalled();
  });
});
