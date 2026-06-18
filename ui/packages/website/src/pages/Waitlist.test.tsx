import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

/*
 * The marketing waitlist page renders Clerk's <Waitlist> inside a
 * <ClerkProvider>. We mock @clerk/clerk-react so the test never reaches
 * Clerk's network/runtime, and serialize the props each receives so we can
 * assert the two things this page exists to control: the publishable key is
 * wired through, and the appearance hides the "Already have access? Sign in"
 * footer row while applying the brand tokens.
 */
vi.mock("@clerk/clerk-react", () => ({
  ClerkProvider: ({
    children,
    publishableKey,
  }: {
    children: React.ReactNode;
    publishableKey: string;
  }) => <div data-clerk-provider={publishableKey}>{children}</div>,
  Waitlist: ({
    appearance,
    afterJoinWaitlistUrl,
  }: {
    appearance: unknown;
    afterJoinWaitlistUrl?: string;
  }) => (
    <div
      data-waitlist={JSON.stringify(appearance)}
      data-after-join={afterJoinWaitlistUrl}
    />
  ),
}));

import WaitlistPage from "./Waitlist";
import { CLERK_PUBLISHABLE_KEY } from "../config";

describe("WaitlistPage", () => {
  it("renders Clerk <Waitlist> inside a ClerkProvider wired to the publishable key", () => {
    const { container } = render(<WaitlistPage />);

    const provider = container.querySelector("[data-clerk-provider]");
    expect(provider).not.toBeNull();
    expect(provider!.getAttribute("data-clerk-provider")).toBe(CLERK_PUBLISHABLE_KEY);
    expect(container.querySelector("[data-waitlist]")).not.toBeNull();
  });

  it("hides the sign-in footer row and applies the brand appearance tokens", () => {
    const { container } = render(<WaitlistPage />);

    const node = container.querySelector("[data-waitlist]")!;
    const appearance = JSON.parse(node.getAttribute("data-waitlist")!);

    // The reason this page is self-hosted: kill the "Already have access?
    // Sign in" row, since nobody has access pre-launch.
    expect(appearance.elements.footerAction).toEqual({ display: "none" });
    // ...while still carrying the dark/mint brand instead of Clerk's palette.
    expect(appearance.variables.colorPrimary).toBe("var(--pulse)");
    expect(appearance.elements.cardBox.backgroundColor).toBe("var(--surface-2)");
  });

  it("returns a joined visitor to the marketing home, not the closed app", () => {
    const { container } = render(<WaitlistPage />);

    const node = container.querySelector("[data-waitlist]")!;
    expect(node.getAttribute("data-after-join")).toBe("/");
  });
});
