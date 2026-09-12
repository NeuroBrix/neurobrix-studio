import { createRawSnippet } from "svelte";
import { expect, test, vi } from "vitest";
import { userEvent } from "vitest/browser";
import { render } from "vitest-browser-svelte";
import Button from "./button.svelte";

const children = createRawSnippet(() => ({ render: () => "<span>Continue</span>" }));

test("renders its content and calls the click callback", async () => {
  const onclick = vi.fn();
  const screen = await render(Button, { children, onclick });
  await screen.getByRole("button", { name: "Continue" }).click();
  expect(onclick).toHaveBeenCalledOnce();
});

test("disabled buttons cannot receive keyboard focus or activate", async () => {
  const onclick = vi.fn();
  const screen = await render(Button, { children, disabled: true, onclick });
  const button = screen.getByRole("button", { name: "Continue" });
  await expect.element(button).toBeDisabled();
  await userEvent.tab();
  await expect.element(button).not.toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(onclick).not.toHaveBeenCalled();
});

test("supports keyboard activation", async () => {
  const onclick = vi.fn();
  const screen = await render(Button, { children, onclick });
  await userEvent.tab();
  await expect.element(screen.getByRole("button", { name: "Continue" })).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(onclick).toHaveBeenCalledOnce();
});
