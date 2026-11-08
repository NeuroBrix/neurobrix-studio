import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { DaemonStatus } from "#lib/services/daemon.js";
import { DaemonLifecycle } from "./daemon.svelte.js";

const stopped: DaemonStatus = { state: "stopped", stale_files: false };
const ready: DaemonStatus = {
  state: "ready",
  model: "TinyLlama-1.1B-Chat-v1.0",
  hardware: "v100-32g",
  family: "llm",
  loaded: true,
  mode: "compiled",
  warm_serving: true,
  engine_version: "0.6.2",
  protocol_version: 1,
  idle_timeout_secs: 1800,
};

describe("DaemonLifecycle", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  test("the poll loop reads once, then only after the interval", async () => {
    const read = vi.fn().mockResolvedValue(stopped);
    const daemon = new DaemonLifecycle(5_000, read);
    daemon.startPolling();
    await vi.advanceTimersByTimeAsync(0);
    expect(read).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(4_999);
    expect(read).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(read).toHaveBeenCalledTimes(2);
  });

  test("dispose stops the loop — leaving the page stops asking", async () => {
    const read = vi.fn().mockResolvedValue(stopped);
    const daemon = new DaemonLifecycle(5_000, read);
    daemon.startPolling();
    await vi.advanceTimersByTimeAsync(0);
    daemon.dispose();
    await vi.advanceTimersByTimeAsync(20_000);
    expect(read).toHaveBeenCalledTimes(1);
  });

  test("a failed refresh keeps the last status and shows the failure beside it", async () => {
    const read = vi
      .fn()
      .mockResolvedValueOnce(stopped)
      .mockRejectedValueOnce(new Error("IPC failed"));
    const daemon = new DaemonLifecycle(5_000, read);
    daemon.startPolling();
    await vi.advanceTimersByTimeAsync(0);
    expect(daemon.status).toEqual(stopped);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(daemon.status).toEqual(stopped);
    expect(daemon.failure).toBe("IPC failed");
  });

  test.each([
    ["stopped", { state: "stopped", stale_files: false } as DaemonStatus, "stopped"],
    ["starting", { state: "starting", model: "m" } as DaemonStatus, "starting"],
    ["ready", ready, "ready"],
    [
      "external daemon",
      {
        state: "external_daemon",
        model: "m",
        hardware: "hw",
        family: undefined,
        loaded: true,
        mode: "compiled",
        warm_serving: true,
        engine_version: "0.6.2",
        protocol_version: 1,
      } as DaemonStatus,
      "external_daemon",
    ],
    ["stopping", { state: "stopping" } as DaemonStatus, "stopping"],
    [
      "a died daemon with an idle timeout",
      { state: "died", expected: true, idle_timeout_secs: 1800 } as DaemonStatus,
      "stopped",
    ],
    [
      "a crashed daemon",
      { state: "died", expected: false, exit_code: 1 } as DaemonStatus,
      "failed",
    ],
    ["an unavailable engine", { state: "unavailable", reason: "none" } as DaemonStatus, "failed"],
    ["a contract failure", { state: "contract_failure", reason: "bad" } as DaemonStatus, "failed"],
  ])("maps the %s to its UI state", (_name, status, expected) => {
    const daemon = new DaemonLifecycle();
    daemon.status = status;
    expect(daemon.lifecycle).toBe(expected);
  });

  test("start calls the service with the model and adopts its status", async () => {
    const startCall = vi.fn().mockResolvedValue(ready);
    const daemon = new DaemonLifecycle(5_000, vi.fn(), startCall);
    daemon.status = stopped;
    await daemon.start("TinyLlama-1.1B-Chat-v1.0");
    expect(startCall).toHaveBeenCalledWith("TinyLlama-1.1B-Chat-v1.0");
    expect(daemon.status).toEqual(ready);
    expect(daemon.failure).toBeNull();
  });

  test("a start rejection surfaces the structured failure, never a retry", async () => {
    const startCall = vi
      .fn()
      .mockRejectedValue({ kind: "conflict", message: "A daemon is already serving." });
    const daemon = new DaemonLifecycle(5_000, vi.fn(), startCall);
    daemon.status = stopped;
    await daemon.start("x");
    expect(daemon.failure).toContain("already serving");
    expect(startCall).toHaveBeenCalledTimes(1);
  });

  test("duplicate submissions are rejected without a second service call", async () => {
    const startCall = vi.fn();
    const daemon = new DaemonLifecycle(5_000, vi.fn(), startCall);
    daemon.status = { state: "starting", model: "m" };
    await daemon.start("other");
    expect(startCall).not.toHaveBeenCalled();
    expect(daemon.failure).toContain("already in progress");
  });

  test("stopping a still-starting daemon is rejected without a service call", async () => {
    const stopCall = vi.fn();
    const daemon = new DaemonLifecycle(5_000, vi.fn(), vi.fn(), stopCall);
    daemon.status = { state: "starting", model: "m" };
    await daemon.stop();
    expect(stopCall).not.toHaveBeenCalled();
    expect(daemon.failure).toContain("already in progress");
  });

  test("stop adopts its status and surfaces its failures", async () => {
    const stopCall = vi.fn().mockResolvedValue({ state: "stopped", stale_files: false });
    const daemon = new DaemonLifecycle(5_000, vi.fn(), vi.fn(), stopCall);
    daemon.status = ready;
    await daemon.stop();
    expect(stopCall).toHaveBeenCalledTimes(1);
    expect(daemon.status).toEqual(stopped);
    expect(daemon.failure).toBeNull();

    const refused = vi
      .fn()
      .mockRejectedValue({ kind: "stop_failed", message: "did not stop within 10 seconds" });
    const stubborn = new DaemonLifecycle(5_000, vi.fn(), vi.fn(), refused);
    stubborn.status = ready;
    await stubborn.stop();
    expect(stubborn.failure).toContain("did not stop");
  });
});
