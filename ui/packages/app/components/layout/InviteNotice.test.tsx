import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { WaitingInvite } from "@/lib/api/invites";

const { pathname } = vi.hoisted(() => ({ pathname: vi.fn<() => string>() }));
vi.mock("next/navigation", () => ({ usePathname: () => pathname() }));

import { INVITES_PATH } from "@/app/(dashboard)/invites/copy";
import { InviteNotice, inviteNoticeText } from "./InviteNotice";
import { accountLabel } from "./workspace-groups";

const FROM_JOHN: WaitingInvite = { id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: 1 };
const FROM_MARY: WaitingInvite = { id: "inv_2", account: { tenant_id: "t_mary", owner_name: "Mary" }, expires_at: 1 };
const NOTICE = "invite-notice";

beforeEach(() => pathname.mockReturnValue("/w/ws_1/fleets"));
afterEach(() => cleanup());

describe("InviteNotice", () => {
  it("should render nothing while no invite waits", () => {
    render(<InviteNotice waiting={[]} />);
    expect(screen.queryByTestId(NOTICE)).toBeNull();
  });

  it("should name the account of a single waiting invite and link to the Invites page", () => {
    render(<InviteNotice waiting={[FROM_JOHN]} />);
    const notice = screen.getByTestId(NOTICE);
    expect(notice.textContent).toContain(accountLabel("John"));
    expect(screen.getByRole("link", { name: "Review" }).getAttribute("href")).toBe(INVITES_PATH);
  });

  it("should count several waiting invites instead of naming one", () => {
    expect(inviteNoticeText([FROM_JOHN, FROM_MARY])).toBe("You have 2 invites waiting.");
  });

  it("should stay hidden on the Invites page and on an invite link, which already show them", () => {
    pathname.mockReturnValue(INVITES_PATH);
    const { rerender } = render(<InviteNotice waiting={[FROM_JOHN]} />);
    expect(screen.queryByTestId(NOTICE)).toBeNull();
    pathname.mockReturnValue(`${INVITES_PATH}/inv_1`);
    rerender(<InviteNotice waiting={[FROM_JOHN]} />);
    expect(screen.queryByTestId(NOTICE)).toBeNull();
  });

  it("should still show on a route that merely starts with the same letters", () => {
    pathname.mockReturnValue(`${INVITES_PATH}-archive`);
    render(<InviteNotice waiting={[FROM_JOHN]} />);
    expect(screen.getByTestId(NOTICE)).toBeTruthy();
  });
});
