import { describe, expect, it } from "vitest";
import { cn } from "./utils.js";

describe("cn", () => {
  it("combines conditional classes without including disabled values", () => {
    expect(cn("base", false, undefined, { selected: true, hidden: false })).toBe("base selected");
  });

  it("lets the caller override conflicting Tailwind utilities", () => {
    expect(cn("px-2 text-sm", "px-4")).toBe("text-sm px-4");
  });
});
