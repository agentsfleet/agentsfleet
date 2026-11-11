import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
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
