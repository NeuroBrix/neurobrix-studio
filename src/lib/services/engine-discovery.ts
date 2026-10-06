import { invoke, isTauri } from "@tauri-apps/api/core";
import * as v from "valibot";

export const engineAvailabilitySchema = v.union([
  v.object({
    state: v.literal("compatible"),
    engine_version: v.string(),
    protocol_version: v.optional(v.string()),
    capabilities: v.optional(v.array(v.string())),
  }),
  v.object({
    state: v.literal("incompatible"),
    engine_version: v.string(),
    supported_range: v.string(),
  }),
  v.object({
    state: v.literal("absent"),
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

export type EngineAvailability = v.InferOutput<typeof engineAvailabilitySchema>;

export const engineDependencyKey = "engine:discovery";

export async function getEngineAvailability(): Promise<EngineAvailability> {
  if (!isTauri()) {
    return {
      state: "unavailable",
      reason: "Engine status is available in the desktop app, not in the browser preview.",
    };
  }

  try {
    const response: unknown = await invoke("engine_discovery");
    const paired = v.safeParse(engineAvailabilitySchema, response);
    if (paired.success) {
      return paired.output;
    }
    return {
      state: "contract_failure",
      reason: "The desktop shell returned an engine status this Studio build does not recognize.",
    };
  } catch (error) {
    return {
      state: "unavailable",
      reason: "Engine status could not be read from the desktop shell",
      diagnostics: error instanceof Error ? error.message : undefined,
    };
  }
}
