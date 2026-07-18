import { describe, expect, it } from "vitest";
import { buildSignInUrl, SIGN_IN_PATH } from "./sign-in-redirect";

describe("buildSignInUrl", () => {
  const ORIGIN = "https://app-dev.agentsfleet.net";

  it("points at the embedded sign-in page on the request origin", () => {
    const url = new URL(buildSignInUrl(`${ORIGIN}/w/ws_1/fleets`, "/w/ws_1/fleets"));
    expect(url.origin).toBe(ORIGIN);
    expect(url.pathname).toBe(SIGN_IN_PATH);
  });

  it("carries the intended destination on redirect_url so a deep-link survives sign-in", () => {
    const url = new URL(buildSignInUrl(`${ORIGIN}/w/ws_1/fleets`, "/w/ws_1/fleets"));
    // Read via URL API (not the raw string) so the assertion is encoding-agnostic.
    expect(url.searchParams.get("redirect_url")).toBe("/w/ws_1/fleets");
  });

  it("passes an already-encoded query string through verbatim (no decode, no double-encode)", () => {
    // `request.nextUrl.search` arrives percent-encoded; the setter must store it
    // as-is so `<SignIn>` gets back the exact string it can navigate to.
    const encoded = "/w/ws_1/fleets?tab=events&q=a%20b";
    const url = new URL(buildSignInUrl(`${ORIGIN}/w/ws_1/fleets`, encoded));
    expect(url.searchParams.get("redirect_url")).toBe(encoded);
  });

  it("handles the dashboard root as a destination", () => {
    const url = new URL(buildSignInUrl(`${ORIGIN}/`, "/"));
    expect(url.pathname).toBe(SIGN_IN_PATH);
    expect(url.searchParams.get("redirect_url")).toBe("/");
  });
});
