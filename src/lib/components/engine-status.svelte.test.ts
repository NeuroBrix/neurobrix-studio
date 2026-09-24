import { expect, test, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import type { EngineAvailability } from "#lib/services/engine-discovery.js";
import EngineStatus from "./engine-status.svelte";

const cases: [name: string, availability: EngineAvailability, expected: RegExp][] = [
  ["compatible", { state: "compatible", engine_version: "0.5.4" }, /engine ready/],
  [
    "incompatible",
    { state: "incompatible", engine_version: "0.7.0", supported_range: ">=0.5.3,<0.6" },
    /outside the tested range/,
  ],
  ["absent", { state: "absent" }, /No NeuroBrix engine found/],
  ["unavailable", { state: "unavailable", reason: "did not respond" }, /did not respond/],
  [
    "contract failure",
    { state: "contract_failure", reason: "unrecognized schema" },
    /unrecognized status/,
  ],
];

test.each(cases)("renders the %s state distinctly", async (_name, availability, expected) => {
  const screen = await render(EngineStatus, { availability, refresh: () => {} });
  await expect.element(screen.getByText(expected)).toBeVisible();
});

test("failure states retry through the page-provided refresh", async () => {
  const refresh = vi.fn();
  const screen = await render(EngineStatus, {
    availability: { state: "unavailable", reason: "did not respond" },
    refresh,
  });
  await screen.getByRole("button", { name: "Check again" }).click();
  expect(refresh).toHaveBeenCalledOnce();
});
