import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { SteerComposer } from "./SteerComposer";

// Mock assistant-ui's ComposerPrimitive so the composer renders without a full
// AssistantRuntimeProvider — the `asChild` Input clones the design-system
// Textarea with the `disabled` + `placeholder` props SteerComposer passes, which
// is exactly the gate under test.
vi.mock("@assistant-ui/react", () => ({
  ComposerPrimitive: {
    Root: ({ children, ...rest }: { children: React.ReactNode }) => <div {...rest}>{children}</div>,
    Input: ({ children, disabled, placeholder }: { children: React.ReactElement; disabled?: boolean; placeholder?: string }) =>
      React.cloneElement(children, { disabled, placeholder } as Record<string, unknown>),
    Send: ({ children }: { children: React.ReactElement }) => children,
  },
}));

afterEach(() => cleanup());

describe("SteerComposer", () => {
  it("test_composer_disabled_while_running", () => {
    const { rerender } = render(<SteerComposer isRunning={true} />);
    const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
    // Mid-run: the composer is disabled and shows the working-state placeholder;
    // there is no interrupt control (steering a running fleet is not a capability).
    expect(textarea.disabled).toBe(true);
    expect(textarea.placeholder).toBe("Fleet is working — composer disabled");
    expect(screen.queryByRole("button", { name: /interrupt|stop|cancel/i })).toBeNull();

    // event_complete flips isRunning false → the composer re-enables.
    rerender(<SteerComposer isRunning={false} />);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).disabled).toBe(false);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).placeholder).toBe("Steer this fleet…");
  });
});
