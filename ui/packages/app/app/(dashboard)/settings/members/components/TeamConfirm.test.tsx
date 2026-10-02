import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { ConfirmDialogProps } from "@agentsfleet/design-system";

// The dialog's props are what is under test: a closing dialog keeps animating
// out after its target clears, and it shows whatever it was last handed.
const { handed } = vi.hoisted(() => ({ handed: vi.fn<(props: ConfirmDialogProps) => void>() }));
vi.mock("@agentsfleet/design-system", () => ({
  ConfirmDialog: (props: ConfirmDialogProps) => {
    handed(props);
    return null;
  },
}));

import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { CONFIRM_KIND, TeamConfirm, type ConfirmTarget } from "./TeamConfirm";

const BOB: MemberSummary = { user_id: "user_bob", display_name: "Bob", email: "bob@example.com", role: ACCOUNT_ROLE.member, joined_at: 1 };
const REMOVE_BOB: ConfirmTarget = { kind: CONFIRM_KIND.remove, member: BOB };

function view(target: ConfirmTarget) {
  return <TeamConfirm target={target} error={null} onOpenChange={vi.fn()} onConfirm={vi.fn()} />;
}

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("TeamConfirm", () => {
  it("should close with the copy it opened with, not a blank title and a default button", () => {
    const { rerender } = render(view(REMOVE_BOB));
    const opened = handed.mock.lastCall?.[0];
    rerender(view(null));
    const closing = handed.mock.lastCall?.[0];

    expect(opened).toMatchObject({ open: true, title: "Remove Bob?", confirmLabel: "Remove" });
    expect(closing).toMatchObject({ open: false, title: opened?.title, confirmLabel: opened?.confirmLabel });
    expect(closing?.description).toBe(opened?.description);
    // Nothing is confirmed while it closes.
    expect(closing?.onConfirm).toBeUndefined();
  });

  it("should render nothing to confirm before any target has opened it", () => {
    render(view(null));
    expect(handed.mock.lastCall?.[0]).toMatchObject({ open: false, onConfirm: undefined });
  });
});
