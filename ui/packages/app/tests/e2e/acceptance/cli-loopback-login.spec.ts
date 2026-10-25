/// <reference types="bun" />

import { readFile, rm, stat } from "node:fs/promises";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { z } from "zod";
// Reuse the terminal harness that the command-line acceptance lane runs.
import { extractLoginUrl, PtyProcess } from "../../../../../../cli/test/acceptance/fixtures/pty";
import { composeEnv } from "../../../../../../cli/test/acceptance/fixtures/cli.js";
import { CLI_CREDENTIAL_PATTERN } from "../../../../../../cli/src/constants/cli-credential";
import { signInAs, fixtureEmail } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { makeCliStateDir, spawnAgentsfleet } from "./fixtures/cli-runner";
import { clientFor } from "./fixtures/api-client";

const LOGIN_TIMEOUT_MS = 60_000;
const CREDENTIALS_NAME = "credentials.json";
const CREDENTIAL_MODE = 0o600;
const MODE_MASK = 0o777;
const COMPARISON_CODE = /Check code:\s*([A-F0-9]{4}-[A-F0-9]{4})/;
const AUTHORIZE = "Authorize";
const DENY = "Deny";
const BUTTON_ROLE = "button";
const credentialSchema = z.object({ token: z.string(), api_url: z.string(), credential_id: z.string() });
const identitySchema = z.object({ email: z.string(), tenant_id: z.string(), user_id: z.string() });

function environment(stateDir: string, dashboard: string): Record<string, string> {
  const api = process.env.NEXT_PUBLIC_API_URL;
  if (!api) throw new Error("NEXT_PUBLIC_API_URL is required");
  return composeEnv({ AGENTSFLEET_API_URL: api, AGENTSFLEET_DASHBOARD_URL: dashboard,
    AGENTSFLEET_STATE_DIR: stateDir, BROWSER: "false", NO_COLOR: "1" });
}

async function loginLink(cli: PtyProcess): Promise<{ url: string; code: string }> {
  const output = await cli.waitForLine(line => COMPARISON_CODE.test(line), LOGIN_TIMEOUT_MS);
  const url = extractLoginUrl(output);
  const code = COMPARISON_CODE.exec(output)?.[1];
  if (!url || !code) throw new Error("The terminal did not announce its login link and comparison code");
  return { url, code };
}

async function assertCredential(stateDir: string, env: Record<string, string>): Promise<void> {
  const filename = join(stateDir, CREDENTIALS_NAME);
  expect((await stat(filename)).mode & MODE_MASK).toBe(CREDENTIAL_MODE);
  const credential = credentialSchema.parse(JSON.parse(await readFile(filename, "utf8")));
  expect(CLI_CREDENTIAL_PATTERN.test(credential.token)).toBe(true);
  expect(credential.api_url).toBe(env.AGENTSFLEET_API_URL);
  const identity = identitySchema.parse(await clientFor({ sessionJwt: credential.token }).get<unknown>("/v1/users/me"));
  expect(identity.email).toBe(fixtureEmail(FIXTURE_KEY.regular));
  const loggedOut = await spawnAgentsfleet(["logout", "--json"], env);
  expect(loggedOut.code).toBe(0);
}

for (const initiallySignedIn of [true, false]) {
  test(`local authorization completes from ${initiallySignedIn ? "an existing" : "a new ticket"} browser sign-in`, async ({ page, baseURL }) => {
    test.setTimeout(LOGIN_TIMEOUT_MS);
    if (!baseURL) throw new Error("The dashboard URL is required");
    const { root, stateDir } = await makeCliStateDir("loopback-login-");
    const env = environment(stateDir, baseURL);
    const cli = PtyProcess.spawnFleetctl(["login"], { env });
    const leaks: string[] = [];
    try {
      if (initiallySignedIn) await signInAs(page, FIXTURE_KEY.regular);
      const link = await loginLink(cli);
      const state = new URLSearchParams(new URL(link.url).hash.slice(1)).get("state");
      if (!state) throw new Error("The local handoff state is missing");
      page.on("request", request => {
        if (request.url().includes(state)) leaks.push("state in request URL");
        if (/posthog|\/capture\/?(?:\?|$)|\/batch\/?(?:\?|$)/i.test(request.url()) &&
          (request.postData() ?? "").includes(state)) leaks.push("state in analytics");
      });
      const response = await page.goto(link.url);
      if (!initiallySignedIn) {
        await expect(page).toHaveURL(/\/sign-in\?/);
        expect(new URL(page.url()).hash).toBe("");
        const destination = new URL(page.url()).searchParams.get("redirect_url");
        if (!destination) throw new Error("The browser lost its sign-in return destination");
        expect(destination.includes(state)).toBe(false);
        // The same Clerk ticket sign-in used by other live acceptance journeys.
        await signInAs(page, FIXTURE_KEY.regular);
        await page.goto(destination);
      } else {
        expect(response?.headers()["referrer-policy"]).toBe("origin");
      }
      await expect(page.getByLabel("Verification code")).toHaveText(link.code);
      await expect(page.getByText(`Signing in as ${fixtureEmail(FIXTURE_KEY.regular)}`)).toBeVisible();
      const callbackRequest = page.waitForRequest(request => new URL(request.url()).hostname === "127.0.0.1" &&
        new URL(request.url()).pathname === "/cli-auth/callback");
      await page.getByRole(BUTTON_ROLE, { name: AUTHORIZE, exact: true }).click();
      const delivered = await callbackRequest;
      expect(delivered.method()).toBe("POST");
      expect(delivered.headers().origin).toBe(new URL(baseURL).origin);
      expect(delivered.headers()["content-type"]).toBe("application/x-www-form-urlencoded");
      await expect(page.getByRole("heading", { name: "Terminal authorized" })).toBeVisible();
      expect(await cli.exited).toBe(0);
      expect(cli.output.includes("Finalizing sign-in…")).toBe(true);
      expect(cli.output.includes("signed in as")).toBe(true);
      expect(cli.output.includes("Enter the 6-digit")).toBe(false);
      expect(leaks).toEqual([]);
      await assertCredential(stateDir, env);
    } finally {
      cli.kill();
      // Logout is idempotent if an earlier assertion interrupted the success path.
      await spawnAgentsfleet(["logout", "--json"], env);
      await rm(root, { recursive: true, force: true });
    }
  });
}

test("denying browser authorization stops the waiting terminal without a saved credential", async ({ page, baseURL }) => {
  if (!baseURL) throw new Error("The dashboard URL is required");
  const { root, stateDir } = await makeCliStateDir("loopback-denied-");
  const cli = PtyProcess.spawnFleetctl(["login"], { env: environment(stateDir, baseURL) });
  try {
    await signInAs(page, FIXTURE_KEY.regular);
    const link = await loginLink(cli);
    await page.goto(link.url);
    await expect(page.getByLabel("Verification code")).toHaveText(link.code);
    await page.getByRole(BUTTON_ROLE, { name: DENY, exact: true }).click();
    expect(await cli.exited).toBe(1);
    expect(await stat(join(stateDir, CREDENTIALS_NAME)).then(() => true, () => false)).toBe(false);
    expect(cli.output.includes("Finalizing sign-in…")).toBe(false);
  } finally {
    cli.kill();
    await rm(root, { recursive: true, force: true });
  }
});

