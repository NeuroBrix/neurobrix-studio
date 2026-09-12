import { $, browser, expect } from "@wdio/globals";
import { describe, it } from "mocha";

describe("built desktop application", () => {
  it("opens Models and Settings and preserves nested routes on reload", async () => {
    await expect($("h1")).toHaveText("NeuroBrix Studio");

    for (const section of ["Models", "Settings"]) {
      await $(`a[href='/workspace/${section.toLowerCase()}']`).click();
      await expect($("h1")).toHaveText(section);
      await browser.refresh();
      await expect($("h1")).toHaveText(section);
      await $("a[href='/']").click();
      await expect($("h1")).toHaveText("NeuroBrix Studio");
      await browser.back();
      await expect($("h1")).toHaveText(section);
      await browser.forward();
      await expect($("h1")).toHaveText("NeuroBrix Studio");
    }
  });
});
