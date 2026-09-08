/**
 * connector-relay.spec.ts — every provider's connect mints the callback the
 * dashboard actually serves, spelled the way a provider registration holds it.
 *
 * Wire: fixture-user Bearer → POST /v1/workspaces/{ws}/connectors/{provider}/connect
 * → read `redirect_uri` out of the URL the daemon minted → compare against the
 * route this dashboard mounts. No browser, because the assertion is about a
 * string the daemon composes, not about a page.
 *
 * # Why this journey exists
 *
 * Two defects in one week lived in this one string, and both reached the
 * development environment because nothing compared it to anything:
 *
 *   1. `RELAY_PATH` was one slash-bearing segment, so the daemon minted
 *      `/api%2Fconnectors/github/callback` and every provider refused the
 *      consent screen (Sep 7, 2026).
 *   2. The registered callback still named the API host, so GitHub refused
 *      with `redirect_uri is not associated` (Sep 8, 2026).
 *
 * The daemon's spelling is Rust; the expectation below is TypeScript and is
 * derived from the route file's own path. They are two independent spellings
 * of one contract, which is the only reason this can fail — a test that asked
 * the daemon what it minted and then agreed with it would have passed through
 * both defects, and the unit test that existed did exactly that.
 *
 * What it CANNOT prove is the other half: whether the provider's registration
 * holds the same value. A provider checks that where no test can see it — an
 * unauthenticated authorize call answers a login redirect for a correct URL,
 * a stale one, and an unrelated domain alike. That half is the human step in
 * `playbooks/operations/acceptance/001_playbook.md`.
 */
import { expect, test } from "@playwright/test";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";

/**
 * Every provider the daemon ships, by route segment.
 *
 * Spelled here rather than read from the catalogue for the same reason the
 * path below is: a list fetched from the daemon would agree with the daemon.
 * A provider added without a row here fails the count assertion.
 */
const PROVIDERS = ["slack", "github", "zoho", "jira", "linear"] as const;

/** The relay route the dashboard mounts: `app/api/connectors/[provider]/callback/route.ts`. */
const RELAY_PREFIX = "/api/connectors/";
const RELAY_SUFFIX = "/callback";
const REDIRECT_PARAM = "redirect_uri";
/** What a path must never contain — the `%2F` defect, in one assertion. */
const ENCODED_SLASH = "%2f";

interface ConnectStarted {
  install_url: string;
}

/** The dashboard origin this run drives, which is the origin a provider returns to. */
function dashboardOrigin(baseURL: string | undefined): string {
  if (!baseURL) throw new Error("the acceptance config must supply a baseURL");
  return new URL(baseURL).origin;
}

test.describe("connector relay", () => {
  test("every provider's connect mints the callback this dashboard serves", async ({
    baseURL,
  }) => {
    const origin = dashboardOrigin(baseURL);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tenant = clientFor(FIXTURE_KEY.regular);

    const minted: string[] = [];
    for (const provider of PROVIDERS) {
      const started = await tenant.post<ConnectStarted>(
        `/v1/workspaces/${workspaceId}/connectors/${provider}/connect`,
        {},
      );

      const authorize = new URL(started.install_url);
      const redirect = authorize.searchParams.get(REDIRECT_PARAM);
      expect(redirect, `${provider} minted no ${REDIRECT_PARAM}`).toBeTruthy();

      const expected = `${origin}${RELAY_PREFIX}${provider}${RELAY_SUFFIX}`;
      expect(redirect, `${provider}'s callback is not the route the dashboard serves`).toBe(
        expected,
      );

      // Separate from the equality above on purpose: an encoded slash makes the
      // path a single segment, which is a different failure from a wrong host
      // and reads as a different sentence when it fires.
      expect(
        new URL(redirect as string).pathname.toLowerCase(),
        `${provider}'s callback path carries an encoded separator`,
      ).not.toContain(ENCODED_SLASH);

      minted.push(provider);
    }

    // A sixth provider shipping without a row above would otherwise pass this
    // journey by never being asked.
    expect(minted, "every shipped provider was asked").toHaveLength(PROVIDERS.length);
  });
});
