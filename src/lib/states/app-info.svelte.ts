import { type AppInfo, AppInfoError, getAppInfo } from "#lib/services/app-info.js";

/** Owned by the Settings panel; Tauri reads cannot be aborted, so discard late results. */
export class AppInfoResource {
  data = $state.raw<AppInfo | undefined>(undefined);
  isLoading = $state(false);
  error = $state.raw<AppInfoError | null>(null);

  #sequence = 0;
  #disposed = false;

  constructor(private readonly read: () => Promise<AppInfo> = getAppInfo) {}

  async load(): Promise<void> {
    if (this.#disposed) return;
    const sequence = ++this.#sequence;
    this.isLoading = true;
    this.error = null;

    try {
      const data = await this.read();
      if (sequence === this.#sequence) this.data = data;
    } catch (cause: unknown) {
      if (sequence === this.#sequence) {
        this.error =
          cause instanceof AppInfoError ? cause : new AppInfoError("request-failed", { cause });
      }
    } finally {
      if (sequence === this.#sequence) this.isLoading = false;
    }
  }

  dispose(): void {
    this.#disposed = true;
    ++this.#sequence;
    this.isLoading = false;
  }
}
