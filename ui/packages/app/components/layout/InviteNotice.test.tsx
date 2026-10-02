import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { alertVariants } from "@agentsfleet/design-system";
import type { WaitingInvite } from "@/lib/api/invites";

const { pathname } = vi.hoisted(() => ({ pathname: vi.fn<() => string>() }));
vi.mock("next/navigation", () => ({ usePathname: () => pathname() }));

import { INVITES_PATH } from "@/app/(dashboard)/invites/copy";
import { InviteNotice, inviteNoticeText } from "./InviteNotice";
import { accountLabel } from "./workspace-groups";

const FROM_JOHN: WaitingInvite = { id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: 1 };
const FROM_MARY: WaitingInvite = { id: "inv_2", account: { tenant_id: "t_mary", owner_name: "Mary" }, expires_at: 1 };
const NOTICE = "invite-notice";
const STRIP_CLASSES = ["rounded-none", "border-x-0", "border-t-0", "px-md", "py-xs"];

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

  // It renders inside the padded canvas, where strip styling (square corners, a
  // bottom rule only) reads as a broken box. Every class of the standard alert
  // must survive, with only the bottom spacing added.
  it("should render as a standard inset alert with the page's bottom spacing, not a full-bleed strip", () => {
    render(<InviteNotice waiting={[FROM_JOHN]} />);
    const notice = screen.getByRole("status");
    expect(notice.dataset.testid).toBe(NOTICE);
    const classes = notice.className.split(" ");
    expect(classes).toEqual(expect.arrayContaining([...alertVariants({ variant: "info" }).split(" "), "mb-lg", "text-sm"]));
    expect(classes.filter((cls) => STRIP_CLASSES.includes(cls))).toEqual([]);
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
