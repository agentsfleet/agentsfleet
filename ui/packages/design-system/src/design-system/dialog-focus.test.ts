import {afterEach, expect, it} from "vitest";
import {waitFor} from "@testing-library/react";
import {focusDialogField} from "./dialog-focus";

afterEach(() => {document.body.replaceChildren();});
function form(html: string) {
  const container = document.createElement("div");
  container.tabIndex = -1;
  container.innerHTML = html;
  document.body.append(container);
  return container;
}
it("focuses the first editable visible field", () => {
  const container = form('<div hidden><input></div><input disabled><input readonly><input type="checkbox"><input aria-label="Name"><textarea></textarea>');
  const event = new Event("open", {cancelable: true});
  expect(focusDialogField(container, event)).toBeNull();
  expect(document.activeElement).toBe(container.querySelector('[aria-label="Name"]'));
  expect(event.defaultPrevented).toBe(true);
});
it("focuses a late enabled field once while initial focus remains on Close", async () => {
  const container = form('<button data-dialog-close>Close</button><input disabled>');
  container.querySelector("button")!.focus();
  const dispose = focusDialogField(container, new Event("open"));
  container.querySelector("input")!.disabled = false;
  await waitFor(() => expect(document.activeElement).toBe(container.querySelector("input")));
  dispose!();
});
it.each(["keydown", "pointerdown"])("does not steal focus after %s interaction", async type => {
  const container = form('<button>Cancel</button>');
  const dispose = focusDialogField(container, new Event("open"));
  container.dispatchEvent(new Event(type, {bubbles: true}));
  container.querySelector("button")!.focus();
  container.append(document.createElement("input"));
  await Promise.resolve();
  expect(document.activeElement).toBe(container.querySelector("button"));
  dispose!();
});
it("waits through unrelated mutations and respects focus outside the dialog", async () => {
  const container = form("");
  const dispose = focusDialogField(container, new Event("open"));
  container.append(document.createElement("p"));
  await Promise.resolve();
  const other = document.createElement("button");
  document.body.append(other);
  other.focus();
  container.append(document.createElement("input"));
  await Promise.resolve();
  expect(document.activeElement).toBe(other);
  dispose!();
});
it.each(["body", "container"])("permits initial focus from %s", async target => {
  const container = form("");
  if (target === "container") container.focus();
  const dispose = focusDialogField(container, new Event("open"));
  container.append(document.createElement("input"));
  await waitFor(() => expect(document.activeElement).toBe(container.querySelector("input")));
  dispose!();
});
it("disconnects when removed before fields arrive", async () => {
  const container = form("");
  const dispose = focusDialogField(container, new Event("open"));
  container.remove();
  container.append(document.createElement("input"));
  await Promise.resolve();
  expect(document.activeElement).toBe(document.body);
  dispose!();
});

it("does not focus an async field after Radix starts the exit animation", async () => {
  const container = form('<button data-dialog-close>Close</button>');
  container.dataset.state = "open";
  const close = container.querySelector("button")!;
  close.focus();
  const dispose = focusDialogField(container, new Event("open"));
  container.dataset.state = "closed";
  container.append(document.createElement("input"));
  await Promise.resolve();
  expect(document.activeElement).toBe(close);
  dispose!();
});

it("cancels delayed initial focus after outside pointer interaction", async () => {
  const container = form('<button data-dialog-close>Close</button>');
  const close = container.querySelector("button")!;
  close.focus();
  const dispose = focusDialogField(container, new Event("open"));
  document.body.dispatchEvent(new Event("pointerdown", {bubbles: true}));
  container.append(document.createElement("input"));
  await Promise.resolve();
  expect(document.activeElement).toBe(close);
  dispose!();
});
