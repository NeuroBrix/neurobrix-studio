import * as v from "valibot";
import { beforeEach, describe, expect, test, vi } from "vitest";
import {
  daemonMutationFailureSchema,
  daemonStatusSchema,
  getDaemonStatus,
  startOwnedDaemon,
  stopOwnedDaemon,
  toMutationFailure,
} from "./daemon.js";

const native = vi.hoisted(() => ({ isTauri: vi.fn(), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: native.isTauri,
  invoke: native.invoke,
}));

const serving = {
  model: "TinyLlama-1.1B-Chat-v1.0",
  hardware: "v100-32g",
  family: "llm",
  loaded: true,
  mode: "compiled",
  warm_serving: true,
  engine_version: "0.5.4",
  protocol_version: 1,
  operations: ["status", "shutdown"],
};

describe("daemonStatusSchema", () => {
  test.each([
    ["stopped", { state: "stopped", stale_files: false }],
    ["stopped with stale files", { state: "stopped", stale_files: true }],
    ["starting", { state: "starting", model: "TinyLlama-1.1B-Chat-v1.0" }],
    ["stopping", { state: "stopping" }],
    ["ready", { state: "ready", idle_timeout_secs: 1800, ...serving }],
    [
      "ready without an optional family",
      { state: "ready", idle_timeout_secs: 1800, ...serving, family: undefined },
    ],
    ["external daemon", { state: "external_daemon", ...serving }],
    [
      "died expected with the invocation's timeout",
      {
        state: "died",
        expected: true,
        idle_timeout_secs: 1800,
        exit_code: 0,
        diagnostics: "[Daemon] Idle timeout (1800s) — shutting down",
      },
    ],
    ["died unexpected with only the required fields", { state: "died", expected: false }],
    ["unavailable", { state: "unavailable", reason: "no engine" }],
    ["contract failure", { state: "contract_failure", reason: "bad stamps" }],
  ])("accepts the documented %s response", (_name, input) => {
    expect(v.parse(daemonStatusSchema, input)).toBeTruthy();
  });

  test.each([
    ["missing state", {}],
    ["unknown state", { state: "ready!" }],
    ["stopped without stale_files", { state: "stopped" }],
    ["starting without a model", { state: "starting" }],
    ["ready missing fields", { state: "ready", model: "x" }],
    [
      "ready with a string protocol",
      { state: "ready", idle_timeout_secs: 1800, ...serving, protocol_version: "1" },
    ],
    ["died without expected", { state: "died", exit_code: 1 }],
    ["unavailable without a reason", { state: "unavailable" }],
    ["wrong family type", { state: "external_daemon", ...serving, family: 7 }],
  ])("rejects malformed response: %s", (_name, input) => {
    const parsed = v.safeParse(daemonStatusSchema, input);
    expect(parsed.success).toBe(false);
  });
});

describe("daemonMutationFailureSchema", () => {
  test.each([
    ["conflict", { kind: "conflict", message: "already serving", engine_version: "0.5.4" }],
    ["conflict without the identity", { kind: "conflict", message: "already running" }],
    ["duplicate submission", { kind: "duplicate_submission", message: "in progress" }],
    ["no owned daemon", { kind: "no_owned_daemon", message: "nothing owned" }],
    ["not ready", { kind: "not_ready", message: "still starting" }],
    ["spawn", { kind: "spawn", message: "could not start", diagnostics: "permission" }],
    [
      "load failed",
      { kind: "load_failed", message: "stopped early", exit_code: 1, diagnostics: "CUDA OOM" },
    ],
    [
      "readiness timeout with excerpts",
      {
        kind: "readiness_timeout",
        message: "did not become ready",
        stdout_excerpt: "[Serve] Loading",
        stderr_excerpt: "downloading",
      },
    ],
    ["contract failure", { kind: "contract_failure", message: "refused" }],
    ["stop failed", { kind: "stop_failed", message: "did not stop within 10 seconds" }],
    ["engine unavailable", { kind: "engine_unavailable", message: "no engine" }],
  ])("accepts the documented %s failure", (_name, input) => {
    expect(v.parse(daemonMutationFailureSchema, input)).toBeTruthy();
  });

  test.each([
    ["unknown kind", { kind: "forced_kill", message: "x" }],
    ["missing message", { kind: "not_ready" }],
    ["wrong engine_version type", { kind: "conflict", message: "x", engine_version: 4 }],
  ])("rejects malformed failure: %s", (_name, input) => {
    const parsed = v.safeParse(daemonMutationFailureSchema, input);
    expect(parsed.success).toBe(false);
  });
});

describe("getDaemonStatus", () => {
  beforeEach(() => vi.clearAllMocks());

  test("validates the Tauri response and returns it", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockResolvedValue({ state: "stopped", stale_files: false });
    expect(await getDaemonStatus()).toEqual({ state: "stopped", stale_files: false });
    expect(native.invoke).toHaveBeenCalledWith("daemon_status");
  });

  test("maps an unrecognizable response to contract_failure", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockResolvedValue({ state: "ready" });
    const status = await getDaemonStatus();
    expect(status.state).toBe("contract_failure");
  });

  test("reports a rejected read as unavailable with diagnostics", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockRejectedValue(new Error("IPC failed"));
    const status = await getDaemonStatus();
    expect(status.state).toBe("unavailable");
    if (status.state === "unavailable") {
      expect(status.diagnostics).toBe("IPC failed");
    }
  });

  test("reports browser preview explicitly instead of faking data", async () => {
    native.isTauri.mockReturnValue(false);
    const status = await getDaemonStatus();
    expect(status.state).toBe("unavailable");
    expect(native.invoke).not.toHaveBeenCalled();
  });
});

describe("the mutation commands", () => {
  beforeEach(() => vi.clearAllMocks());

  test("start passes the model id and returns the validated status", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockResolvedValue({ state: "stopping" });
    expect(await startOwnedDaemon("TinyLlama-1.1B-Chat-v1.0")).toEqual({ state: "stopping" });
    expect(native.invoke).toHaveBeenCalledWith("start_owned_daemon", {
      modelId: "TinyLlama-1.1B-Chat-v1.0",
    });
  });

  test("start propagates the structured failure for the caller to surface", async () => {
    native.isTauri.mockReturnValue(true);
    const failure = { kind: "conflict", message: "already serving", engine_version: "0.5.4" };
    native.invoke.mockRejectedValue(failure);
    await expect(startOwnedDaemon("x")).rejects.toEqual(failure);
    expect(toMutationFailure(failure)).toEqual(failure);
  });

  test("start maps an unrecognizable success to contract_failure", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockResolvedValue({ state: "ready" });
    const status = await startOwnedDaemon("x");
    expect(status.state).toBe("contract_failure");
  });

  test("start refuses the browser preview explicitly", async () => {
    native.isTauri.mockReturnValue(false);
    await expect(startOwnedDaemon("x")).rejects.toThrow("desktop app");
    expect(native.invoke).not.toHaveBeenCalled();
  });

  test("stop returns the validated status", async () => {
    native.isTauri.mockReturnValue(true);
    native.invoke.mockResolvedValue({ state: "stopped", stale_files: false });
    expect(await stopOwnedDaemon()).toEqual({ state: "stopped", stale_files: false });
    expect(native.invoke).toHaveBeenCalledWith("stop_owned_daemon");
  });

  test("stop refuses the browser preview explicitly", async () => {
    native.isTauri.mockReturnValue(false);
    await expect(stopOwnedDaemon()).rejects.toThrow("desktop app");
    expect(native.invoke).not.toHaveBeenCalled();
  });

  test("toMutationFailure returns undefined for an unrecognized rejection", () => {
    expect(toMutationFailure("IPC failed")).toBeUndefined();
    expect(toMutationFailure({ kind: "forced_kill" })).toBeUndefined();
  });
});
