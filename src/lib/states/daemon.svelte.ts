import {
  type DaemonStatus,
  getDaemonStatus,
  startOwnedDaemon,
  stopOwnedDaemon,
  toMutationFailure,
} from "#lib/services/daemon.js";

/** The lifecycle the UI renders, derived from the manager's status —
 * never invented. A died daemon is an expected stop (the idle timeout or
 * an external stop) or a failure (a crash); an engine that cannot answer
 * is a failure with its reason. */
export type DaemonLifecycleState =
  | "stopped"
  | "starting"
  | "ready"
  | "stopping"
  | "failed"
  | "external_daemon";

/**
 * The Workspace panel's view of the daemon: reads poll, mutations are
 * explicit user actions, and failures are surfaced, never retried and
 * never swallowed. The next poll is scheduled only after the previous
 * read finishes; disposal stops the loop, so leaving the page stops
 * asking.
 */
export class DaemonLifecycle {
  status = $state.raw<DaemonStatus | undefined>(undefined);
  failure = $state.raw<string | null>(null);
  isMutating = $state(false);

  #pollHandle: ReturnType<typeof setTimeout> | undefined;
  #disposed = false;
  #polling = false;

  constructor(
    private readonly pollIntervalMs = 5_000,
    private readonly read: () => Promise<DaemonStatus> = getDaemonStatus,
    private readonly startCall: (modelId: string) => Promise<DaemonStatus> = startOwnedDaemon,
    private readonly stopCall: () => Promise<DaemonStatus> = stopOwnedDaemon,
  ) {}

  /** Begin the bounded poll loop. Idempotent. */
  startPolling(): void {
    if (this.#disposed || this.#pollHandle !== undefined) return;
    void this.refresh();
  }

  /** Stop polling and ignore everything after; leaving the page stops
   * asking the engine anything. */
  dispose(): void {
    this.#disposed = true;
    if (this.#pollHandle !== undefined) clearTimeout(this.#pollHandle);
    this.#pollHandle = undefined;
  }

  /** Read-only: a read never starts or stops anything. A failed refresh
   * keeps the last successful status and shows the failure beside it. */
  async refresh(): Promise<void> {
    if (this.#disposed || this.#polling) return;
    this.#polling = true;
    try {
      this.status = await this.read();
      this.failure = null;
    } catch (error) {
      this.failure = messageOf(error);
    } finally {
      this.#polling = false;
    }
    this.#scheduleNext();
  }

  #scheduleNext(): void {
    if (this.#disposed || this.#pollHandle !== undefined) return;
    this.#pollHandle = setTimeout(() => {
      this.#pollHandle = undefined;
      void this.refresh();
    }, this.pollIntervalMs);
  }

  /** The lifecycle the UI renders, derived from the reported status. */
  get lifecycle(): DaemonLifecycleState | undefined {
    switch (this.status?.state) {
      case "stopped":
        return "stopped";
      case "starting":
        return "starting";
      case "ready":
        return "ready";
      case "external_daemon":
        return "external_daemon";
      case "stopping":
        return "stopping";
      case "died":
        return this.status.expected ? "stopped" : "failed";
      case "unavailable":
      case "contract_failure":
        return "failed";
      default:
        return undefined;
    }
  }

  /** Start an owned daemon for an explicit model. Explicit user action
   * only: duplicate submissions are rejected with a message (never
   * queued), and a rejection is surfaced with the engine's own
   * diagnostics, never silently retried. */
  async start(modelId: string): Promise<void> {
    if (this.#rejectWhenBusy("A daemon start or stop is already in progress.")) return;
    this.isMutating = true;
    try {
      this.status = await this.startCall(modelId);
      this.failure = null;
    } catch (error) {
      this.#surface(error);
    } finally {
      this.isMutating = false;
      this.#scheduleNext();
    }
  }

  /** Stop the daemon this session owns — the command refuses anything it
   * does not own, and so does this. */
  async stop(): Promise<void> {
    if (this.#rejectWhenBusy("A daemon stop is already in progress.")) return;
    this.isMutating = true;
    try {
      this.status = await this.stopCall();
      this.failure = null;
    } catch (error) {
      this.#surface(error);
    } finally {
      this.isMutating = false;
      this.#scheduleNext();
    }
  }

  #rejectWhenBusy(message: string): boolean {
    if (this.#disposed) return true;
    if (this.isMutating || this.status?.state === "starting" || this.status?.state === "stopping") {
      this.failure = message;
      return true;
    }
    return false;
  }

  /** A structured failure's message, or the raw error's text — the
   * engine's own diagnostics, carried to the person reading them. */
  #surface(error: unknown): void {
    const failure = toMutationFailure(error);
    this.failure = failure ? failure.message || failure.kind : messageOf(error);
  }
}

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "The operation failed for an unknown reason.";
}
