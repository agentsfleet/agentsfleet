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
