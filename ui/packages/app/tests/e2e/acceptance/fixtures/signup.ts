/**
 * signUpAs(page, email, password) — drive Clerk's browser-side SignUp SDK
 * directly, without touching the hosted SignUp component.
 *
 * The hosted SignUp form renders a Cloudflare Turnstile widget on the
 * email/password step. `setupClerkTestingToken` forces `captcha_bypass:
 * true` on every Frontend API (FAPI) response and attaches the testing
 * token as a query param, but the form's own browser-side bot-check
 * still gates navigation to the One-Time Password (OTP) screen — so a
 * UI-driven signup hangs waiting for `input[autocomplete="one-time-code"]`
 * that never renders.
 *
 * Calling `Clerk.client.signUp.create` directly skips the form entirely.
 * The FAPI calls still go through the testing-token interceptor, which
 * keeps the captcha bypass in place, and Clerk development test-mode OTP
 * shortcut (`424242` for `+clerk_test@…` aliases) still works on
 * `attemptEmailAddressVerification`. After zombied bootstraps the tenant
 * and Clerk metadata catches up, the helper activates the signup-created
 * session so clerk-js owns the browser session cookies.
 */
import type { Page } from "@playwright/test";
import { setupClerkTestingToken } from "@clerk/testing/playwright";
import { bootstrapTenant } from "./bootstrap";
import { findUserIdByEmail, refreshSessionToken, waitForTenantMetadata } from "./clerk-admin";
import { FIXTURE_KEY, type FixtureKey } from "./constants";

const TEST_OTP = "424242";
const SESSION_METADATA_TIMEOUT_MS = 30_000;
const SESSION_METADATA_POLL_MS = 1_000;
const NO_WORKSPACE_TEXT = /no workspace yet/i;
const WORKSPACES_PATH = "/v1/tenants/me/workspaces";

interface ClerkSignUpAttempt {
  createdSessionId?: string | null;
  status?: string;
}

// Narrow shape of window.Clerk we touch from page.evaluate. We don't re-declare
// the global type (@clerk/clerk-js publishes its own augmentation and TS2717's
// "subsequent property declarations must match" forbids us from narrowing it).
// Instead we cast inside the browser context.
interface ClerkBrowserSurface {
  loaded: boolean;
  client: {
    signUp: {
      create: (params: { emailAddress: string; password: string }) => Promise<unknown>;
      prepareEmailAddressVerification: (params: { strategy: string }) => Promise<unknown>;
      attemptEmailAddressVerification: (params: { code: string }) => Promise<ClerkSignUpAttempt>;
    };
  };
  setActive: (params: { session: string }) => Promise<void>;
  signOut: () => Promise<void>;
}

interface SignUpOptions {
  bootstrap?: boolean;
  key?: FixtureKey;
  requireWorkspaceSession?: boolean;
}

interface SignUpSession {
  sessionJwt: string;
  workspaceId?: string;
}

export async function signUpAs(
  page: Page,
  email: string,
  password: string,
  options: SignUpOptions = {},
): Promise<SignUpSession> {
  const key = options.key ?? FIXTURE_KEY.regular;
  const shouldBootstrap = options.bootstrap ?? true;
  await setupClerkTestingToken({ page });
  await page.goto("/sign-up");
  await page.waitForFunction(() => {
    const clerk = (window as unknown as { Clerk?: { loaded?: boolean } }).Clerk;
    return Boolean(clerk?.loaded);
  });
  const sessionId = await page.evaluate(
    async ({ emailAddress, pwd, code }) => {
      const clerk = (window as unknown as { Clerk: ClerkBrowserSurface }).Clerk;
      await clerk.client.signUp.create({ emailAddress, password: pwd });
      await clerk.client.signUp.prepareEmailAddressVerification({ strategy: "email_code" });
      const attempt = await clerk.client.signUp.attemptEmailAddressVerification({ code });
      if (!attempt.createdSessionId) {
        throw new Error(`signUp.attempt did not return createdSessionId (status=${attempt.status})`);
      }
      return attempt.createdSessionId;
    },
    { emailAddress: email, pwd: password, code: TEST_OTP },
  );
  const clerkUserId = await findUserIdByEmail(email);
  if (!clerkUserId) {
    throw new Error(`signed-up Clerk user not found for ${email}`);
  }
  if (shouldBootstrap) {
    await bootstrapTenant({ key, email, password, clerkUserId });
  }
  await waitForTenantMetadata(clerkUserId);
  await page.evaluate(async (id) => {
    const clerk = (window as unknown as { Clerk: ClerkBrowserSurface }).Clerk;
    await clerk.setActive({ session: id });
  }, sessionId);
  const sessionJwt = await refreshSessionToken(sessionId);
  if (options.requireWorkspaceSession) {
    return {
      sessionJwt,
      workspaceId: await waitForTenantWorkspace(page, email, sessionJwt),
    };
  }
  return { sessionJwt };
}

async function waitForTenantWorkspace(page: Page, email: string, sessionJwt: string): Promise<string> {
  const deadline = Date.now() + SESSION_METADATA_TIMEOUT_MS;
  let lastApiState = "not checked";
  while (Date.now() < deadline) {
    await page.goto("/zombies");
    await page.getByRole("heading", { name: /zombies/i }).first().waitFor({ timeout: 5_000 });
    const apiState = await readWorkspaceApiState(page, sessionJwt);
    lastApiState = apiState.label;
    if ((await page.getByText(NO_WORKSPACE_TEXT).count()) === 0 && apiState.workspaceId) {
      return apiState.workspaceId;
    }
    await new Promise((resolve) => setTimeout(resolve, SESSION_METADATA_POLL_MS));
  }
  throw new Error(
    `signed-up Clerk session for ${email} did not resolve a tenant workspace (${lastApiState})`,
  );
}

interface WorkspaceApiState {
  label: string;
  workspaceId?: string;
}

async function readWorkspaceApiState(page: Page, sessionJwt: string): Promise<WorkspaceApiState> {
  const apiUrl = process.env.NEXT_PUBLIC_API_URL;
  if (!apiUrl) return { label: "NEXT_PUBLIC_API_URL missing" };

  const res = await page.request.get(`${apiUrl}${WORKSPACES_PATH}`, {
    headers: { Authorization: `Bearer ${sessionJwt}` },
  });
  if (!res.ok()) return { label: `workspace API ${res.status()}` };
  const body = (await res.json()) as { items?: Array<{ id?: unknown }> };
  const workspaceId = typeof body.items?.[0]?.id === "string" ? body.items[0].id : undefined;
  return { label: `workspace API items=${body.items?.length ?? "unknown"}`, workspaceId };
}
