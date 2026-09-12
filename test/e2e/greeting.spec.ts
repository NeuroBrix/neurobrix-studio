import { $, browser, expect } from "@wdio/globals";
import { describe, it } from "mocha";

describe("built desktop application", () => {
  it("submits to the real Rust command before and after reload", async () => {
    for (const name of ["Studio", "Reload"]) {
      await $("#greet-input").setValue(name);
      await $("button[type='submit']").click();
      await expect($("main > p:last-of-type")).toHaveText(
        `Hello, ${name}! You've been greeted from Rust!`,
      );
      if (name === "Studio") await browser.refresh();
    }
  });
});
