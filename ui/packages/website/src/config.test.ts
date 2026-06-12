import { afterEach, describe, expect, it, vi } from "vitest";

/*
 * APP_BASE_URL is resolved once at module-evaluation time from three inputs in
 * precedence order: an explicit VITE_APP_BASE_URL override, then the prod host
 * when building for production, else the dev host. The build-target branch
 * (import.meta.env.PROD) only takes its prod arm in a production build, which
 * the test runtime never is — so we stub the env and re-import the module to
 * exercise every arm.
 */

const PROD_HOST = "https://app.usezombie.com";
const DEV_HOST = "https://app.dev.usezombie.com";

async function loadAppBaseUrl(): Promise<string> {
  vi.resetModules();
  const mod = await import("./config");
  return mod.APP_BASE_URL;
}

describe("APP_BASE_URL resolution", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.resetModules();
  });

  it("prefers a trimmed VITE_APP_BASE_URL override over the build-target hosts", async () => {
    vi.stubEnv("VITE_APP_BASE_URL", "  https://app.staging.usezombie.com  ");
    vi.stubEnv("PROD", true);
    expect(await loadAppBaseUrl()).toBe("https://app.staging.usezombie.com");
  });

  it("uses the production host when building for prod with no override", async () => {
    vi.stubEnv("VITE_APP_BASE_URL", "");
    vi.stubEnv("PROD", true);
    expect(await loadAppBaseUrl()).toBe(PROD_HOST);
  });

  it("uses the dev host when not building for prod and no override", async () => {
    vi.stubEnv("VITE_APP_BASE_URL", "");
    vi.stubEnv("PROD", false);
    expect(await loadAppBaseUrl()).toBe(DEV_HOST);
  });
});

/*
 * agentsfleet rebrand pins. Two directions, both deliberate:
 *  - operational strings that must NOT change until their own cutover
 *    spec lands (installer domain, GitHub org URL, team mailbox);
 *  - parked flips (docs host) that must not drift to agentsfleet.net
 *    before the host answers. Unpinning is the conscious act of the
 *    cutover edit, never a side effect.
 */
describe("rebrand pins — operational strings stay until their cutover", () => {
  it("install command stays on the usezombie.sh installer verbatim", async () => {
    vi.resetModules();
    const mod = await import("./config");
    expect(mod.INSTALL_COMMAND).toBe("curl -fsSL https://usezombie.sh | bash");
  });

  it("GitHub URL stays on the unchanged org", async () => {
    vi.resetModules();
    const mod = await import("./config");
    expect(mod.GITHUB_URL).toBe("https://github.com/usezombie/usezombie");
  });

  it("team email stays on the routed mailbox", async () => {
    vi.resetModules();
    const mod = await import("./config");
    expect(mod.TEAM_EMAIL).toBe("team@usezombie.com");
  });

  it("docs URL is parked on the serving host until docs.agentsfleet.net answers", async () => {
    vi.resetModules();
    const mod = await import("./config");
    expect(mod.DOCS_URL).toBe("https://docs.usezombie.com");
  });
});
