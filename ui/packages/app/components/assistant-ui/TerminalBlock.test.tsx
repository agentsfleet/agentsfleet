import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TerminalBlock } from "./TerminalBlock";

afterEach(cleanup);

it("does not fabricate a successful exit for a completed command with an unknown outcome", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={[]} done />);
  expect(container.querySelector('[data-slot="terminal-block"]')?.getAttribute("data-state")).toBe("unconfirmed");
  expect(screen.getByText("Outcome unconfirmed")).toBeTruthy();
  expect(container.textContent).not.toContain("exit 0");
});

it("keeps interruption separate from completion", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={["interrupted"]} done outcome="interrupted" exitCode={130} />);
  expect(container.querySelector('[data-slot="terminal-block"]')?.getAttribute("data-state")).toBe("interrupted");
  expect(screen.getByText("exit 130")).toBeTruthy();
});

it("shows a recorded failure and exit code", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={["permission denied"]} done outcome="failed" exitCode={2} />);
  expect(container.querySelector('[data-slot="terminal-block"]')?.getAttribute("data-state")).toBe("failed");
  expect(screen.getByText("exit 2")).toBeTruthy();
  expect(screen.getByText("permission denied")).toBeTruthy();
});

it("keeps a confirmed success without a recorded exit code distinct from exit zero", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={["ready"]} done outcome="succeeded" />);
  expect(screen.getByText("Completed")).toBeTruthy();
  expect(container.textContent).not.toContain("exit 0");
});

it("renders terminal control sequences and markup without creating executable elements", () => {
  const {container} = render(<TerminalBlock command="<script>unsafe</script>" lines={["\u001b[31mdenied\u001b[0m", "<img src=x onerror=unsafe>"]} done outcome="failed" />);
  expect(screen.getByText("denied")).toBeTruthy();
  expect(screen.getByText("<img src=x onerror=unsafe>")).toBeTruthy();
  expect(container.querySelector("script, img")).toBeNull();
});

it("copies plain text without terminal escapes", async () => {
  const user = userEvent.setup();
  const write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  render(<TerminalBlock command="check" lines={["\u001b[1;2;32mready\u001b[0m", "second"]} done exitCode={0} truncated />);
  expect(screen.getByText("output truncated")).toBeTruthy();
  await user.click(screen.getByRole("button", {name: "Copy output"}));
  expect(write).toHaveBeenCalledExactlyOnceWith("ready\nsecond");
});

it("keeps running output visible and treats nonzero exits as failures without a supplied outcome", () => {
  const view = render(<TerminalBlock command="check" lines={["first"]} done={false} />);
  expect(view.container.querySelector('[data-state="running"]')).toBeTruthy();
  expect(screen.getByText("first")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Copy output"})).toBeNull();
  view.rerender(<TerminalBlock command="check" lines={["first"]} done exitCode={3} />);
  expect(view.container.querySelector('[data-state="failed"]')).toBeTruthy();
  expect(screen.getByText("exit 3")).toBeTruthy();
});

it("keeps explicit interruption and failure visible when no exit code was recorded", () => {
  const view = render(<TerminalBlock command="check" lines={["first", "second"]} done outcome="interrupted" />);
  expect(screen.getByText("Interrupted")).toBeTruthy();
  view.rerender(<TerminalBlock command="check" lines={[]} done outcome="failed" />);
  expect(screen.getByText("Failed")).toBeTruthy();
});
