import { expect, test, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import type { DaemonStatus } from "#lib/services/daemon.js";
import { DaemonLifecycle } from "#lib/states/daemon.svelte.js";
import DaemonPanel from "./daemon-panel.svelte";

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

/** A panel wired to a scripted lifecycle: the poll reads the given
 * status, and the tests hold the mutation mocks. */
function scripted(status: DaemonStatus) {
  const read = vi.fn().mockImplementation(async () => status);
  const startCall = vi.fn();
  const stopCall = vi.fn();
  const daemon = new DaemonLifecycle(60_000, read, startCall, stopCall);
  daemon.status = status;
  return { daemon, read, startCall, stopCall };
}

test("renders the ready state with the reported serving summary and a stop button", async () => {
  const { daemon } = scripted(ready);
  const screen = await render(DaemonPanel, { daemon });
  await expect
    .element(screen.getByText(/Serving TinyLlama-1\.1B-Chat-v1\.0 on v100-32g/))
    .toBeVisible();
  await expect.element(screen.getByRole("button", { name: "Stop daemon" })).toBeVisible();
});

test("renders an external daemon with its identity and never offers to stop it", async () => {
  const { daemon } = scripted({
    state: "external_daemon",
    model: "TinyLlama-1.1B-Chat-v1.0",
    hardware: "v100-32g",
    family: undefined,
    loaded: true,
    mode: "compiled",
    warm_serving: true,
    engine_version: "0.6.2",
    protocol_version: 1,
  });
  const screen = await render(DaemonPanel, { daemon });
  await expect
    .element(screen.getByText(/A daemon this Studio session did not start/))
    .toBeVisible();
  expect(screen.container.querySelector("button")).toBeNull();
});

test("renders the stopped state with the start form", async () => {
  const { daemon } = scripted({ state: "stopped", stale_files: false });
  const screen = await render(DaemonPanel, { daemon });
  await expect.element(screen.getByText("No daemon is running.")).toBeVisible();
  await expect.element(screen.getByRole("button", { name: "Start daemon" })).toBeVisible();
});

test("the start form submits the typed model id as the explicit start action", async () => {
  const { daemon, startCall } = scripted({ state: "stopped", stale_files: false });
  const screen = await render(DaemonPanel, { daemon });
  await screen.getByRole("textbox").fill("TinyLlama-1.1B-Chat-v1.0");
  await screen.getByRole("button", { name: "Start daemon" }).click();
  expect(startCall).toHaveBeenCalledWith("TinyLlama-1.1B-Chat-v1.0");
});

test("stale files from a dead daemon are reported, not hidden", async () => {
  const { daemon } = scripted({ state: "stopped", stale_files: true });
  const screen = await render(DaemonPanel, { daemon });
  await expect.element(screen.getByText(/files from a previous one remain/)).toBeVisible();
});

test("an expected death reports the recorded idle timeout", async () => {
  const { daemon } = scripted({
    state: "died",
    expected: true,
    idle_timeout_secs: 1800,
  });
  const screen = await render(DaemonPanel, { daemon });
  await expect.element(screen.getByText(/after 1800 s idle/)).toBeVisible();
});

test("a crash renders the failure path with its diagnostics and a retry", async () => {
  const { daemon, read } = scripted({
    state: "died",
    expected: false,
    exit_code: 1,
    diagnostics: "RuntimeError: CUDA out of memory",
  });
  const screen = await render(DaemonPanel, { daemon });
  await expect.element(screen.getByText("The daemon stopped unexpectedly.")).toBeVisible();
  await expect.element(screen.getByText("Exit code 1.")).toBeVisible();
  await expect.element(screen.getByText("RuntimeError: CUDA out of memory")).toBeVisible();
  await screen.getByRole("button", { name: "Check again" }).click();
  expect(read).toHaveBeenCalled();
});

test("an unavailable engine renders its reason and a retry", async () => {
  const { daemon } = scripted({
    state: "unavailable",
    reason: "No NeuroBrix engine was found on this computer.",
  });
  const screen = await render(DaemonPanel, { daemon });
  await expect.element(screen.getByText(/No NeuroBrix engine was found/)).toBeVisible();
  await expect.element(screen.getByRole("button", { name: "Check again" })).toBeVisible();
});

test("the poll failure is announced beside the status", async () => {
  const daemon = new DaemonLifecycle(
    60_000,
    vi.fn().mockImplementation(async () => {
      throw new Error("IPC failed");
    }),
  );
  const screen = await render(DaemonPanel, { daemon });
  await expect.element(screen.getByRole("alert")).toHaveTextContent("IPC failed");
});
