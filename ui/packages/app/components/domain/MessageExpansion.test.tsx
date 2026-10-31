import { afterEach, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MessageExpansion } from "./MessageExpansion";

afterEach(cleanup);

const TEXT = `${"Evidence with timestamps. ".repeat(60)}The final evidence.`;

it("lets readers expand and collapse a historical message without losing its text", () => {
  render(<MessageExpansion length={TEXT.length}>{display => <p>{display(TEXT)}</p>}</MessageExpansion>);
  expect(screen.queryByText(/The final evidence/)).toBeNull();
  const more = screen.getByRole("button", {name: "Show more"});
  expect(more.getAttribute("aria-expanded")).toBe("false");
  expect(more.querySelector('svg[aria-hidden="true"]')).toBeTruthy();
  fireEvent.click(more);
  expect(screen.getByText(TEXT)).toBeTruthy();
  const less = screen.getByRole("button", {name: "Show less"});
  expect(less.getAttribute("aria-expanded")).toBe("true");
  expect(document.getElementById(less.getAttribute("aria-controls")!)).toBeTruthy();
  fireEvent.click(less);
  expect(screen.queryByText(/The final evidence/)).toBeNull();
});

it.each([0, 999, 1000])("keeps short or empty messages intact (%s)", length => {
  const text = "a".repeat(length);
  const {container} = render(<MessageExpansion length={length}>{display => <p>{display(text)}</p>}</MessageExpansion>);
  expect(container.textContent).toBe(text);
  expect(screen.queryByRole("button")).toBeNull();
});

it("keeps a live reply open when it settles, then allows folding", () => {
  const view = render(<MessageExpansion length={TEXT.length} running>{display => <p>{display(TEXT)}</p>}</MessageExpansion>);
  expect(screen.getByText(TEXT)).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
  view.rerender(<MessageExpansion length={TEXT.length}>{display => <p>{display(TEXT)}</p>}</MessageExpansion>);
  expect(screen.getByText(TEXT)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Show less"}));
  expect(screen.queryByText(/The final evidence/)).toBeNull();
});
