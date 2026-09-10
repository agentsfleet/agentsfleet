import { SAMPLE_TEMPLATES } from "./harness";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { authMock as auth } from "../helpers/dashboard-mocks";
import { listWorkspaceFleetLibraryMock, listSecretsMock } from "../helpers/dashboard-app-mocks";

const LIBRARY_WRITE_SCOPE = "library:write";
// The affordance's own on-screen label. It is the ONLY user-visible proof that
// the capability survived the claims() seam: a wrong shape at that seam removes
// this button with no error, no log and no other change to the page.
const ADD_LIBRARY_AFFORDANCE_LABEL = "Create fleet library";

/** A session token whose claim set carries `scopes`, the way Clerk projects it. */
function sessionWithScopes(scopes: string | null) {
  auth.mockResolvedValue({
    getToken: vi.fn().mockResolvedValue("token_abc"),
    userId: "usr_1",
    sessionClaims: scopes === null ? null : { scopes },
  });
}

async function renderGallery() {
  const { InstallFleetData } =
    await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
  return renderToStaticMarkup(
    React.createElement(
      React.Fragment,
      null,
      await InstallFleetData({ workspaceId: "ws_1", query: {} }),
    ),
  );
}

// The install page reads its claim set through `claims()` and hands the RESULT
// to `hasLibraryWriteScope`. That hand-off is the whole gate: `claims()` returns
// the claim set, and a regression returning the session WRAPPER (an object whose
// only interesting key is `sessionClaims`) reads as "no scopes" — the
// add-library-entry affordance disappears for every operator, silently. These
// tests assert the affordance's presence as a function of what that seam yields.
describe("install page — library:write claims seam", () => {
  it("shows the add-library-entry affordance when the session's claim set carries library:write", async () => {
    sessionWithScopes(`fleet:read ${LIBRARY_WRITE_SCOPE}`);
    listWorkspaceFleetLibraryMock.mockResolvedValue({
      items: SAMPLE_TEMPLATES,
      next_cursor: null,
      total: null,
    });
    listSecretsMock.mockResolvedValue({ secrets: [] });

    const markup = await renderGallery();

    expect(markup).toContain("GitHub PR reviewer"); // the gallery really rendered
    expect(markup).toContain(ADD_LIBRARY_AFFORDANCE_LABEL);
  });

  it("hides the affordance when the same claim set lacks library:write", async () => {
    sessionWithScopes("fleet:read");
    listWorkspaceFleetLibraryMock.mockResolvedValue({
      items: SAMPLE_TEMPLATES,
      next_cursor: null,
      total: null,
    });
    listSecretsMock.mockResolvedValue({ secrets: [] });

    const markup = await renderGallery();

    expect(markup).toContain("GitHub PR reviewer");
    expect(markup).not.toContain(ADD_LIBRARY_AFFORDANCE_LABEL);
  });

  it("hides the affordance and invites nothing when the session carries no claim set at all", async () => {
    // An unavailable provider and an anonymous session both land here, and
    // `claims()` yields null for both: fail closed, and do not tell a viewer
    // who cannot add an entry to add one.
    sessionWithScopes(null);
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: [] });
    listSecretsMock.mockResolvedValue({ secrets: [] });

    const markup = await renderGallery();

    expect(markup).not.toContain(ADD_LIBRARY_AFFORDANCE_LABEL);
    expect(markup).toContain("Ask a workspace admin to add one."); // pin test: literal is the contract
  });

  it("offers the affordance from the empty state too when the claim set carries library:write", async () => {
    sessionWithScopes(LIBRARY_WRITE_SCOPE);
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: [] });
    listSecretsMock.mockResolvedValue({ secrets: [] });

    const markup = await renderGallery();

    expect(markup).toContain(ADD_LIBRARY_AFFORDANCE_LABEL);
    expect(markup).toContain("Write your own fleet library."); // pin test: literal is the contract
  });

  it("never satisfies the gate from a nested sessionClaims — the wrapper is not a claim set", async () => {
    // The differential half of the seam, asserted on the gate itself: the SAME
    // scopes string grants when it sits on the claim set and grants nothing when
    // it sits one level down inside a wrapper. Tolerating the wrapper here would
    // paper over a `claims()` regression and leave the real defect in place, so
    // this pins the intolerance rather than the symptom.
    const { hasLibraryWriteScope } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/scope");
    const claimSet = { scopes: `fleet:read ${LIBRARY_WRITE_SCOPE}` };

    expect(hasLibraryWriteScope(claimSet)).toBe(true);
    expect(hasLibraryWriteScope({ sessionClaims: claimSet })).toBe(false);
  });
});
