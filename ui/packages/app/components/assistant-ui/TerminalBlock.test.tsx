import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TerminalBlock } from "./TerminalBlock";

afterEach(cleanup);

it("does not fabricate a successful exit for a completed command with an unknown outcome", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={[]} visibleCount={0} done />);
  expect(container.querySelector('[data-slot="terminal-block"]')?.getAttribute("data-state")).toBe("unconfirmed");
  expect(screen.getByText("Outcome unconfirmed")).toBeTruthy();
  expect(container.textContent).not.toContain("exit 0");
});

it("keeps interruption separate from completion", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={["interrupted"]} visibleCount={1} done outcome="interrupted" exitCode={130} />);
  expect(container.querySelector('[data-slot="terminal-block"]')?.getAttribute("data-state")).toBe("interrupted");
  expect(screen.getByText("exit 130")).toBeTruthy();
});

it("shows a recorded failure and exit code", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={["permission denied"]} visibleCount={1} done outcome="failed" exitCode={2} />);
  expect(container.querySelector('[data-slot="terminal-block"]')?.getAttribute("data-state")).toBe("failed");
  expect(screen.getByText("exit 2")).toBeTruthy();
  expect(screen.getByText("permission denied")).toBeTruthy();
});

it("keeps a confirmed success without a recorded exit code distinct from exit zero", () => {
  const {container} = render(<TerminalBlock command="check-release" lines={["ready"]} visibleCount={1} done outcome="succeeded" />);
  expect(screen.getByText("Completed")).toBeTruthy();
  expect(container.textContent).not.toContain("exit 0");
});

it("renders terminal control sequences and markup without creating executable elements", () => {
  const {container} = render(<TerminalBlock command="<script>unsafe</script>" lines={["\u001b[31mdenied\u001b[0m", "<img src=x onerror=unsafe>"]} visibleCount={2} done outcome="failed" />);
  expect(screen.getByText("denied")).toBeTruthy();
  expect(screen.getByText("<img src=x onerror=unsafe>")).toBeTruthy();
  expect(container.querySelector("script, img")).toBeNull();
});

it.each([[999.8, "999ms"], [1000, "1s"], [1234, "1.2s"], [61_100, "1m 01s"], [NaN, "0ms"]])("shows the recorded duration %s as %s", (durationMs, label) => {
  render(<TerminalBlock command="check" lines={[]} visibleCount={0} done exitCode={0} durationMs={durationMs} />);
  expect(screen.getByText(label)).toBeTruthy();
});

it("reveals collapsed output on request and copies plain text without terminal escapes", async () => {
  const user = userEvent.setup();
  const write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  render(<TerminalBlock variant="ink" command="check" cwd="/release" lines={["\u001b[1;2;32mready\u001b[0m", "second"]}
    stderr={["warning"]} visibleCount={3} done exitCode={0} maxCollapsedLines={1} truncated />);
  expect(screen.queryByText("second")).toBeNull();
  expect(screen.getByText("output truncated")).toBeTruthy();
  await user.click(screen.getByRole("button", {name: "Show all 3 lines"}));
  expect(screen.getByText("second")).toBeTruthy();
  expect(screen.getByText("warning")).toBeTruthy();
  await user.click(screen.getByRole("button", {name: "Show less"}));
  expect(screen.queryByText("warning")).toBeNull();
  await user.click(screen.getByRole("button", {name: "Copy output"}));
  expect(write).toHaveBeenCalledExactlyOnceWith("ready\nsecond\nwarning");
});

it("keeps running output partial and treats nonzero exits as failures without a supplied outcome", () => {
  const view = render(<TerminalBlock variant="ink" command="check" cwd="/release" lines={["first", "second"]} visibleCount={1} done={false} />);
  expect(view.container.querySelector('[data-state="running"]')).toBeTruthy();
  expect(screen.queryByText("second")).toBeNull();
  view.rerender(<TerminalBlock variant="ink" command="check" lines={["first"]} visibleCount={1} done exitCode={3} />);
  expect(view.container.querySelector('[data-state="failed"]')).toBeTruthy();
  expect(screen.getByText("exit 3")).toBeTruthy();
});

it("keeps explicit interruption and failure visible when no exit code was recorded", () => {
  const view = render(<TerminalBlock variant="ink" command="check" lines={["first", "second"]} stderr={["warning"]}
    visibleCount={3} done outcome="interrupted" />);
  expect(screen.getByText("Interrupted")).toBeTruthy();
  expect(screen.getByText("warning")).toBeTruthy();
  view.rerender(<TerminalBlock command="check" lines={[]} visibleCount={0} done outcome="failed" />);
  expect(screen.getByText("Failed")).toBeTruthy();
});

it("keeps the working directory, duration and truncation notice readable in both surfaces", () => {
  const view = render(<TerminalBlock command="check" cwd="/release" lines={["partial"]} visibleCount={1} done={false} truncated />);
  expect(screen.getByText("/release $")).toBeTruthy();
  expect(screen.getByText("output truncated")).toBeTruthy();
  view.rerender(<TerminalBlock variant="ink" command="check" lines={[]} visibleCount={0} done durationMs={1000} />);
  expect(screen.getByText("1s")).toBeTruthy();
  expect(screen.getByText("Outcome unconfirmed")).toBeTruthy();
});
