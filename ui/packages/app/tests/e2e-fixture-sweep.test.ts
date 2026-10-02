import { describe, expect, it } from "vitest";
import { PER_RUN_FIXTURE_RE } from "./e2e/acceptance/global-teardown";
import { TEST_INBOX_DOMAIN, TEST_INBOX_LOCAL_PREFIX } from "./e2e/acceptance/fixtures/constants";

// The teardown sweep deletes users, so its pattern is its safety boundary: it
// must match every address a per-run spec mints and nothing a person could own.
const TAG = "0a1b2c3d";

describe("the stale fixture sweep's address pattern", () => {
  it("should match a team invitee minted in Resend's test inbox", () => {
    expect(PER_RUN_FIXTURE_RE.test(`${TEST_INBOX_LOCAL_PREFIX}${TAG}@${TEST_INBOX_DOMAIN}`)).toBe(true);
  });

  it("should leave the rest of that inbox's domain alone", () => {
    expect(PER_RUN_FIXTURE_RE.test(`delivered@${TEST_INBOX_DOMAIN}`)).toBe(false);
    expect(PER_RUN_FIXTURE_RE.test(`${TEST_INBOX_LOCAL_PREFIX}${TAG}@${TEST_INBOX_DOMAIN}.example`)).toBe(false);
    expect(PER_RUN_FIXTURE_RE.test(`${TEST_INBOX_LOCAL_PREFIX}not-a-tag@${TEST_INBOX_DOMAIN}`)).toBe(false);
  });

  it("should still match the signups' addresses and the invitees earlier runs left", () => {
    expect(PER_RUN_FIXTURE_RE.test(`signup-fixture-${TAG}+clerk_test@e2e.agentsfleet.net`)).toBe(true);
    expect(PER_RUN_FIXTURE_RE.test(`team-invitee-${TAG}+clerk_test@e2e.agentsfleet.net`)).toBe(true);
    expect(PER_RUN_FIXTURE_RE.test("admin-fixture@e2e.agentsfleet.net")).toBe(false);
  });
});
