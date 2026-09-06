import { afterEach, describe, expect, it, vi } from "vitest";

/*
 * WAITLIST_URL is resolved once at module-evaluation time from three inputs in
 * precedence order: an explicit VITE_WAITLIST_URL override, then the prod host
 * when building for production, else the dev host. The build-target branch
 * (import.meta.env.PROD) only takes its prod arm in a production build, which
 * the test runtime never is — so we stub the env and re-import the module to
 * exercise every arm.
 */

const PROD_HOST = "https://accounts.agentsfleet.net/waitlist";
const DEV_HOST = "https://winning-wombat-65.accounts.dev/waitlist";

async function loadWaitlistUrl(): Promise<string> {
  vi.resetModules();
  const mod = await import("./config");
  return mod.WAITLIST_URL;
}

describe("WAITLIST_URL resolution", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.resetModules();
  });

  it("prefers a trimmed VITE_WAITLIST_URL override over the build-target hosts", async () => {
    vi.stubEnv("VITE_WAITLIST_URL", "  https://accounts.staging.agentsfleet.net/waitlist  ");
    vi.stubEnv("PROD", true);
    expect(await loadWaitlistUrl()).toBe("https://accounts.staging.agentsfleet.net/waitlist");
  });

  it("uses the production host when building for prod with no override", async () => {
    vi.stubEnv("VITE_WAITLIST_URL", "");
    vi.stubEnv("PROD", true);
    expect(await loadWaitlistUrl()).toBe(PROD_HOST);
  });

  it("uses the dev host when not building for prod and no override", async () => {
    vi.stubEnv("VITE_WAITLIST_URL", "");
    vi.stubEnv("PROD", false);
    expect(await loadWaitlistUrl()).toBe(DEV_HOST);
  });
});

/*
 * agentsfleet rebrand pins. Two directions, both deliberate:
 *  - operational strings that must NOT change until their own cutover
 *    spec lands (team mailbox);
 *  - flipped values (installer domain, docs host, GitHub org) that must
 *    not regress to the retired brand. Unpinning either direction is the
 *    conscious act of a cutover edit, never a side effect.
 */
describe("rebrand pins — flipped values must not regress; operational strings stay", () => {
  it("GitHub URL serves on the renamed agentsfleet/agentsfleet repo", async () => {
    vi.resetModules();
    const mod = await import("./config");
    expect(mod.GITHUB_URL).toBe("https://github.com/agentsfleet/agentsfleet");
  });

  it("docs URL serves on the agentsfleet host", async () => {
    vi.resetModules();
    const mod = await import("./config");
    expect(mod.DOCS_URL).toBe("https://docs.agentsfleet.net");
  });
});
