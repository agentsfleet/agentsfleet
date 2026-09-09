import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

const { resolvePeopleActionMock } = vi.hoisted(() => ({
  resolvePeopleActionMock: vi.fn(),
}));

vi.mock("@/app/actions/identity", () => ({
  resolvePeopleAction: resolvePeopleActionMock,
}));

import { PersonLabel } from "@/components/domain/PersonLabel";
import { resetPersonDirectory } from "@/lib/identity/person-directory";

const ALICE = "user_3HizL5hdEfQ9Gy4e6Qsuq9nkKCu";
const BOB = "user_2AbcD5hdEfQ9Gy4e6Qsuq9nkZZZ";
const SWEEPER = "system:approval_gate_sweeper";

beforeEach(() => {
  resetPersonDirectory();
  resolvePeopleActionMock.mockResolvedValue({ [ALICE]: "Ada Lovelace", [BOB]: "Bob Vance" });
});

afterEach(() => {
  cleanup();
  resolvePeopleActionMock.mockReset();
});

describe("PersonLabel", () => {
  it("shows the shortened subject first, then the name the directory returns", async () => {
    render(<PersonLabel actor={ALICE} />);
    // The cell is never blank: a row recording who decided still says so while
    // the lookup is in flight.
    expect(screen.getByText("user_3HizL…kKCu")).toBeTruthy();
    expect(await screen.findByText("Ada Lovelace")).toBeTruthy();
  });

  // The name is for the human; the subject is what matches a log line or an
  // API response, so it stays reachable rather than being replaced.
  it("keeps the full subject on hover", async () => {
    render(<PersonLabel actor={ALICE} />);
    const label = await screen.findByText("Ada Lovelace");
    expect(label.getAttribute("title")).toBe(ALICE);
  });

  it("asks once for a table that renders the same subject many times", async () => {
    render(
      <>
        <PersonLabel actor={ALICE} />
        <PersonLabel actor={ALICE} />
        <PersonLabel actor={BOB} />
      </>,
    );
    await screen.findAllByText("Ada Lovelace");
    expect(resolvePeopleActionMock).toHaveBeenCalledTimes(1);
    expect(resolvePeopleActionMock.mock.calls[0]![0]).toEqual([ALICE, BOB]);
  });

  it("does not ask again for a subject already answered", async () => {
    render(<PersonLabel actor={ALICE} />);
    await screen.findByText("Ada Lovelace");
    cleanup();
    render(<PersonLabel actor={ALICE} />);
    expect(screen.getByText("Ada Lovelace")).toBeTruthy();
    expect(resolvePeopleActionMock).toHaveBeenCalledTimes(1);
  });

  // Clerk has never heard of the daemon's sentinels; sending one buys a
  // round-trip for a 404.
  it("names a daemon sentinel itself and never asks the directory", () => {
    render(<PersonLabel actor={SWEEPER} />);
    expect(screen.getByText("Auto-swept")).toBeTruthy();
    expect(resolvePeopleActionMock).not.toHaveBeenCalled();
  });

  it("leaves the fallback standing when the directory refuses", async () => {
    resolvePeopleActionMock.mockRejectedValue(new Error("clerk is down"));
    render(<PersonLabel actor={ALICE} />);
    await vi.waitFor(() => expect(resolvePeopleActionMock).toHaveBeenCalled());
    expect(screen.getByText("user_3HizL…kKCu")).toBeTruthy();
  });

  it("renders nothing for an unattributed row", () => {
    const { container } = render(<PersonLabel actor="" />);
    expect(container.textContent).toBe("");
    expect(resolvePeopleActionMock).not.toHaveBeenCalled();
  });
});
