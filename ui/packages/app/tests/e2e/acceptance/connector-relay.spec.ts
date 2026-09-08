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
 * Every provider the daemon ships TODAY, by route segment.
 *
 * Not the list under test — the list under test is the daemon's own catalogue,
 * read at runtime, so a sixth provider is covered the day it ships. This array
 * is the floor: it asserts the catalogue has not silently LOST one, which a
 * catalogue-derived loop alone cannot see (an empty catalogue would pass it).
 * The two together mean neither a new provider nor a vanished one goes unasked.
 */
const KNOWN_PROVIDERS = ["slack", "github", "zoho", "jira", "linear"] as const;

/** The relay route the dashboard mounts: `app/api/connectors/[provider]/callback/route.ts`. */
const RELAY_PREFIX = "/api/connectors/";
const RELAY_SUFFIX = "/callback";
const REDIRECT_PARAM = "redirect_uri";
/** What a path must never contain — the `%2F` defect, in one assertion. */
const ENCODED_SLASH = "%2f";

interface ConnectStarted {
  install_url: string;
}

/** One row of `GET …/connectors` — the daemon's own list of what it ships. */
interface CatalogueEntry {
  id: string;
  configured: boolean;
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

    const catalogue = await tenant.get<CatalogueEntry[]>(
      `/v1/workspaces/${workspaceId}/connectors`,
    );
    const shipped = catalogue.map((entry) => entry.id);
    // The floor, not the subject: a catalogue that lost a provider would leave
    // a loop over itself perfectly green.
    for (const known of KNOWN_PROVIDERS) {
      expect(shipped, `the catalogue no longer lists ${known}`).toContain(known);
    }

    // A provider this DEPLOYMENT has no app bag for refuses connect with
    // UZ-CONN-001, correctly — asking it would grade the environment, not the
    // callback. The floor above already proved none of them vanished.
    const connectable = catalogue.filter((entry) => entry.configured).map((entry) => entry.id);
    expect(connectable.length, "no provider is configured here; the journey would prove nothing")
      .toBeGreaterThan(0);

    const minted: string[] = [];
    for (const provider of connectable) {
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

    // Against the CATALOGUE, never against the array this file spells: comparing
    // a list built from `KNOWN_PROVIDERS` to its own length is a tautology, and
    // a sixth provider would have passed it by never being asked.
    expect(minted, "every configured provider the daemon lists was asked").toEqual(connectable);
  });
});
