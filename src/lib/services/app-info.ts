import { invoke, isTauri } from "@tauri-apps/api/core";
import * as v from "valibot";

const nonBlank = v.pipe(
  v.string(),
  v.check((value) => value.trim().length > 0),
);

// Extra fields are deliberately stripped; only this app-information contract enters state.
const AppInfoSchema = v.object({
  version: nonBlank,
  os: nonBlank,
  architecture: nonBlank,
});

export type AppInfo = v.InferOutput<typeof AppInfoSchema>;
type AppInfoErrorCode = "unavailable" | "invalid-response" | "request-failed";

const messages: Record<AppInfoErrorCode, string> = {
  unavailable: "App information is available in the desktop app. Open NeuroBrix Studio to view it.",
  "invalid-response":
    "Studio returned incomplete or invalid app information. Try again or reopen the app.",
  "request-failed": "Studio could not read its app information. Try again or reopen the app.",
};

export class AppInfoError extends Error {
  constructor(
    readonly code: AppInfoErrorCode,
    options?: ErrorOptions,
  ) {
    super(messages[code], options);
    this.name = "AppInfoError";
  }
}

export async function getAppInfo(): Promise<AppInfo> {
  if (!isTauri()) throw new AppInfoError("unavailable");

  let response: unknown;
  try {
    response = await invoke<unknown>("get_app_info");
  } catch (cause: unknown) {
    throw new AppInfoError("request-failed", { cause });
  }

  const result = v.safeParse(AppInfoSchema, response);
  if (!result.success) throw new AppInfoError("invalid-response", { cause: result.issues });
  return result.output;
}
