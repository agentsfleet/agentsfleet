import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {act, cleanup, fireEvent, render, screen, within} from "@testing-library/react";
import {renderToString} from "react-dom/server";
import {Button} from "@agentsfleet/design-system";
import {ChatErrorNotice} from "@/components/domain/shared/chat/ChatErrorNotice";

const TITLE = "We couldn’t complete that";
const DESCRIPTION = "We couldn’t load your model choices. Try again in a moment.";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {cleanup(); vi.useRealTimers();});

it("expires the flash while retaining the failed operation and retry", async () => {
  const retry = vi.fn();
  render(<ChatErrorNotice title={TITLE} actions={<Button onClick={retry}>Retry</Button>}>{DESCRIPTION}</ChatErrorNotice>);
  const notice = screen.getByRole("status", {name: "Chat error notice"});
  await act(async () => {vi.advanceTimersByTime(6_000);});
  expect(notice.getAttribute("aria-hidden")).toBe("true");
  expect(screen.getByRole("alert").textContent).toContain(DESCRIPTION);
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  expect(retry).toHaveBeenCalledOnce();
});

it.each(["hover", "focus"])("keeps the flash available during %s and closes it without removing recovery", async (interaction) => {
  render(<ChatErrorNotice title={TITLE} actions={<Button>Retry</Button>}>{DESCRIPTION}</ChatErrorNotice>);
  const notice = screen.getByRole("status", {name: "Chat error notice"});
  const dismiss = within(notice).getByRole("button", {name: "Dismiss error notice"});
  if (interaction === "hover") fireEvent.mouseEnter(notice); else fireEvent.focus(dismiss);
  await act(async () => {vi.advanceTimersByTime(10_000);});
  expect(notice.getAttribute("aria-hidden")).toBe("false");
  if (interaction === "hover") fireEvent.mouseLeave(notice); else fireEvent.blur(dismiss, {relatedTarget: document.body});
  fireEvent.click(dismiss);
  expect(notice.getAttribute("aria-hidden")).toBe("true");
  expect(screen.getByRole("alert").textContent).toContain(DESCRIPTION);
  expect(screen.getByRole("button", {name: "Retry"})).toBeTruthy();
});

it("uses the application canvas and releases its timer on unmount", () => {
  const canvas = document.createElement("div"); canvas.className = "app-dashboard-canvas"; document.body.append(canvas);
  try {
    const view = render(<ChatErrorNotice title={TITLE} actions={<Button>Retry</Button>}>{DESCRIPTION}</ChatErrorNotice>);
    expect(screen.getByRole("status", {name: "Chat error notice"}).parentElement).toBe(canvas);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
  } finally {canvas.remove();}
});

it("renders recovery on the server before mounting the flash", () => {
  expect(renderToString(<ChatErrorNotice title={TITLE} actions={<Button>Retry</Button>}>{DESCRIPTION}</ChatErrorNotice>)).toContain(DESCRIPTION);
});
