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
 * keeps the captcha bypass in place, and Clerk DEV's test-mode OTP
 * shortcut (`424242` for `+clerk_test@…` aliases) still works on
 * `attemptEmailAddressVerification`. `setActive` writes the same
 * cookie shape clerk-js writes for an interactive signup — `azp`-bearing
 * session JWT, `__client_uat`, `__clerk_db_jwt` — so clerkMiddleware
 * treats the resulting session like any other.
 */
import type { Page } from "@playwright/test";
import { setupClerkTestingToken } from "@clerk/testing/playwright";

const TEST_OTP = "424242";

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
}

export async function signUpAs(page: Page, email: string, password: string): Promise<void> {
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
  await page.evaluate(async (id) => {
    const clerk = (window as unknown as { Clerk: ClerkBrowserSurface }).Clerk;
    await clerk.setActive({ session: id });
  }, sessionId);
}
