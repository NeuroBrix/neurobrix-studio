import { invoke, isTauri } from "@tauri-apps/api/core";
import * as v from "valibot";

/**
 * The serving fields a live daemon reports (engine `get_status` +
 * `daemon_identity`), exactly as Rust serializes them — optional fields are
 * absent when the engine had nothing to say, never invented.
 */
const serving = {
  model: v.string(),
  hardware: v.string(),
  family: v.optional(v.string()),
  loaded: v.boolean(),
  mode: v.string(),
  warm_serving: v.boolean(),
  engine_version: v.string(),
  protocol_version: v.number(),
  operations: v.array(v.string()),
} as const;

/** What answers on Studio's engine instance — one explicit state per value. */
export const daemonStatusSchema = v.union([
  v.object({ state: v.literal("stopped"), stale_files: v.boolean() }),
  v.object({ state: v.literal("starting"), model: v.string() }),
  v.object({ state: v.literal("stopping") }),
  v.object({
    state: v.literal("ready"),
    idle_timeout_secs: v.number(),
    ...serving,
  }),
  v.object({ state: v.literal("external_daemon"), ...serving }),
  v.object({
    state: v.literal("died"),
    expected: v.boolean(),
    idle_timeout_secs: v.optional(v.number()),
    exit_code: v.optional(v.number()),
    diagnostics: v.optional(v.string()),
  }),
  v.object({
    state: v.literal("unavailable"),
    reason: v.string(),
    diagnostics: v.optional(v.string()),
  }),
  v.object({
    state: v.literal("contract_failure"),
    reason: v.string(),
    diagnostics: v.optional(v.string()),
  }),
]);

export type DaemonStatus = v.InferOutput<typeof daemonStatusSchema>;

/** The structured failures a mutation command can reject with — validated
 * here too, never swallowed, never silently retried. */
export const daemonMutationFailureSchema = v.union([
  v.object({
    kind: v.literal("conflict"),
    message: v.string(),
    engine_version: v.optional(v.string()),
  }),
  v.object({ kind: v.literal("duplicate_submission"), message: v.string() }),
  v.object({ kind: v.literal("no_owned_daemon"), message: v.string() }),
  v.object({ kind: v.literal("not_ready"), message: v.string() }),
  v.object({
    kind: v.literal("spawn"),
    message: v.string(),
    diagnostics: v.optional(v.string()),
  }),
  v.object({
    kind: v.literal("load_failed"),
    message: v.string(),
    exit_code: v.optional(v.number()),
    diagnostics: v.optional(v.string()),
  }),
  v.object({
    kind: v.literal("readiness_timeout"),
    message: v.string(),
    stdout_excerpt: v.optional(v.string()),
    stderr_excerpt: v.optional(v.string()),
  }),
  v.object({ kind: v.literal("contract_failure"), message: v.string() }),
  v.object({ kind: v.literal("stop_failed"), message: v.string() }),
  v.object({
    kind: v.literal("engine_unavailable"),
    message: v.string(),
    diagnostics: v.optional(v.string()),
  }),
]);

export type DaemonMutationFailure = v.InferOutput<typeof daemonMutationFailureSchema>;

const STATUS_CONTRACT_FAILURE: DaemonStatus = {
  state: "contract_failure",
  reason: "The desktop shell returned a daemon status this Studio build does not recognize.",
};

function parseStatus(response: unknown): DaemonStatus {
  const parsed = v.safeParse(daemonStatusSchema, response);
  return parsed.success ? parsed.output : STATUS_CONTRACT_FAILURE;
}

/** Read-only: what answers on the instance. A read never starts or stops
 * anything; every failure is an explicit state, never faked data. */
export async function getDaemonStatus(): Promise<DaemonStatus> {
  if (!isTauri()) {
    return {
      state: "unavailable",
      reason: "Daemon status is available in the desktop app, not in the browser preview.",
    };
  }
  try {
    const response: unknown = await invoke("daemon_status");
    return parseStatus(response);
  } catch (error) {
    return {
      state: "unavailable",
      reason: "Daemon status could not be read from the desktop shell",
      diagnostics: errorText(error),
    };
  }
}

/** Start an owned daemon for an explicit model id. Rejections carry the
 * structured failure; an unrecognizable success is reported as a contract
 * failure, and the next status read reconciles the truth. */
export async function startOwnedDaemon(modelId: string): Promise<DaemonStatus> {
  if (!isTauri()) {
    throw new Error(
      "The engine daemon can be started from the desktop app, not the browser preview.",
    );
  }
  const response: unknown = await invoke("start_owned_daemon", { modelId });
  return parseStatus(response);
}

/** Stop the daemon this session owns — the command refuses anything it
 * does not own, on the Rust side. */
export async function stopOwnedDaemon(): Promise<DaemonStatus> {
  if (!isTauri()) {
    throw new Error(
      "The engine daemon can be stopped from the desktop app, not the browser preview.",
    );
  }
  const response: unknown = await invoke("stop_owned_daemon");
  return parseStatus(response);
}

/** Tauri rejects a mutation with the serialized structured failure; this
 * validates it, and returns `undefined` when it is not recognizable. */
export function toMutationFailure(error: unknown): DaemonMutationFailure | undefined {
  const parsed = v.safeParse(daemonMutationFailureSchema, error);
  return parsed.success ? parsed.output : undefined;
}

function errorText(error: unknown): string | undefined {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return undefined;
}
