import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** One queue per shell, so rapid navigation cannot leave an older native title behind. */
export class WindowTitle {
  #pending = Promise.resolve();

  update(title: string): Promise<void> {
    const update = this.#pending.then(async () => {
      if (isTauri()) await getCurrentWindow().setTitle(title);
    });

    // Keep later navigation usable after failure; the caller still receives the rejection.
    this.#pending = update.catch(() => {});
    return update;
  }
}
