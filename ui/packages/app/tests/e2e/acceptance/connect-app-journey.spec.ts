/**
 * connect-app-journey.spec.ts — "connect an app, then disconnect it."
 *
 * A workspace holding a Linear connection sees it connected, manages it and
 * disconnects it; Connect then sends the browser to the provider's consent
 * screen carrying this dashboard's own callback; and a failed callback lands
 * on a page that explains itself and leads back. The consent screen itself is
 * the provider's and stays a human step (see connector-relay.spec.ts), so the
 * provider host is answered locally instead of being visited.
 *
 * Slack runs whichever journey the deployment supports, read from the same
 * catalogue the page renders. Configured, it walks Linear's path with a seeded
 * Slack connection. Unconfigured (the local acceptance bootstrap stores only
 * `github-app`), the row offers Request access instead of Connect, and the
 * daemon refuses the connect the page does not offer. The other case is
 * reported as skipped, with the reason, never passed.
 */
import { expect, test } from "@playwright/test";
import type { ConnectorCatalogEntry } from "@/lib/api/connectors";
import { signInAs } from "./fixtures/auth";
import { clientFor, FixtureHttpError } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { CONNECTOR_SERVICE_SLACK, ensureConnectorHandle, GRANT_CREDENTIAL_NAME, SLACK_HANDLE, SLACK_TEAM_NAME } from "./fixtures/grants";
import { ensureSecondWorkspace } from "./fixtures/seed";
import { workspaceHref } from "./fixtures/nav";

const JOURNEY_WORKSPACE = "fixture-browser-connect-journey";
const PROVIDER = "Linear";
const PROVIDER_CONSENT = "https://linear.app/**";
const ACTION_TIMEOUT_MS = 15_000;
const SLACK = "Slack";
const SLACK_CONSENT = "https://slack.com/**";
// `afd_api_tenant` answers a connect this deployment cannot serve as an operator's fault.
const CONNECTOR_NOT_CONFIGURED = "UZ-CONN-001";
const UNAVAILABLE = 503;

async function slackEntry(workspaceId: string): Promise<ConnectorCatalogEntry> {
  const catalogue = await clientFor(FIXTURE_KEY.regular).get<ConnectorCatalogEntry[]>(`/v1/workspaces/${workspaceId}/connectors`);
  const entry = catalogue.find(row => row.id === CONNECTOR_SERVICE_SLACK);
  if (!entry) throw new Error("the connector catalogue no longer lists Slack");
  return entry;
}

test.describe("connect an app", () => {
  let workspaceId = "";

  test.afterAll(async () => {
    if (!workspaceId) return;
    await clientFor(FIXTURE_KEY.regular).delete(`/v1/workspaces/${workspaceId}/secrets/${GRANT_CREDENTIAL_NAME}`).catch(() => undefined);
  });

  test("a connected app is managed, disconnected, reconnected through consent, and a failed callback explains itself", async ({ page, baseURL }) => {
    workspaceId = (await ensureSecondWorkspace(FIXTURE_KEY.regular, JOURNEY_WORKSPACE)).id;
    await ensureConnectorHandle(FIXTURE_KEY.regular, workspaceId);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, "integrations"));
    const row = page.getByTestId("integration-linear");
    await expect(row).toContainText("Connected");
    const manage = row.getByRole("button", { name: `Manage ${PROVIDER}`, exact: true });
    await expect(manage, "the deployment must configure the Linear app (sync the linear-app bag)").toBeVisible();

    // Manage → Disconnect removes the workspace's authorization.
    await manage.click();
    const dialog = page.getByRole("dialog", { name: PROVIDER });
    await expect(dialog).toContainText("Authorization saved");
    await dialog.getByRole("button", { name: `Disconnect ${PROVIDER}`, exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });
    await expect(row).toContainText("Not connected");
    const stored = await clientFor(FIXTURE_KEY.regular).get<{ secrets: { name: string }[] }>(`/v1/workspaces/${workspaceId}/secrets`);
    expect(stored.secrets.map(secret => secret.name)).not.toContain(GRANT_CREDENTIAL_NAME);

    // Connect leaves for the provider's consent screen with this dashboard's callback.
    await page.route(PROVIDER_CONSENT, route => route.fulfill({ status: 200, contentType: "text/html", body: "<title>Provider consent</title>" }));
    await row.getByRole("button", { name: `Connect ${PROVIDER}`, exact: true }).click();
    await page.waitForURL(/^https:\/\/linear\.app\//, { timeout: ACTION_TIMEOUT_MS });
    const callback = new URL(new URL(page.url()).searchParams.get("redirect_uri") ?? "", "https://invalid.example");
    expect(callback.origin, "consent must return to this dashboard").toBe(new URL(baseURL ?? "").origin);
    expect(callback.pathname).toMatch(/\/connectors\/linear\/callback$/);

    // A failed callback explains itself, echoes nothing, and leads back to Connected apps.
    const integrations = `/w/${workspaceId}/integrations`;
    await page.goto(`/connectors/complete?reason=denied&return_to=${encodeURIComponent(integrations)}&code=provider-echo`);
    await expect(page.getByRole("heading", { name: "Connection could not finish" })).toBeVisible();
    await expect(page.getByText("Authorization was cancelled.", { exact: false })).toBeVisible();
    await expect(page.getByText("provider-echo")).toHaveCount(0);
    await page.getByRole("link", { name: "Back to Connect apps" }).click();
    await expect(page).toHaveURL(new RegExp(`${integrations}$`));
    await page.goto(`/connectors/complete?return_to=${encodeURIComponent("https://evil.example/w/x/integrations")}`);
    await expect(page.getByRole("link", { name: "Back to dashboard" })).toHaveAttribute("href", "/");
  });
});

test.describe("connect Slack", () => {
  let workspaceId = "";

  test.afterAll(async () => {
    if (!workspaceId) return;
    await clientFor(FIXTURE_KEY.regular).delete(`/v1/workspaces/${workspaceId}/secrets/${SLACK_HANDLE.name}`).catch(() => undefined);
  });

  test("an unconfigured Slack offers Request access instead of Connect, and the daemon refuses its connect", async ({ page }) => {
    workspaceId = (await ensureSecondWorkspace(FIXTURE_KEY.regular, JOURNEY_WORKSPACE)).id;
    test.skip((await slackEntry(workspaceId)).configured, "this deployment configures the Slack app; the configured journey runs instead");
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, "integrations"));
    const row = page.getByTestId("integration-slack");
    await expect(row).toContainText(SLACK);
    await expect(row).toContainText("Requires platform setup");
    await expect(row.getByRole("button", { name: `Connect ${SLACK}`, exact: true })).toHaveCount(0);
    await row.getByRole("button", { name: `Request access to ${SLACK}`, exact: true }).click();
    await expect(page.getByTestId("workspace-toast")).toHaveText("Requested");

    // The connect the page withholds is refused by the daemon too, not merely hidden.
    const refused: unknown = await clientFor(FIXTURE_KEY.regular)
      .post(`/v1/workspaces/${workspaceId}/connectors/${CONNECTOR_SERVICE_SLACK}/connect`, {})
      .then(() => null, (error: unknown) => error);
    expect(refused).toBeInstanceOf(FixtureHttpError);
    expect(refused).toMatchObject({ status: UNAVAILABLE, message: expect.stringContaining(CONNECTOR_NOT_CONFIGURED) });
    expect((await slackEntry(workspaceId)).connected).toBe(false);
  });

  test("a configured Slack connection is managed, disconnected, and reconnected through consent", async ({ page, baseURL }) => {
    workspaceId = (await ensureSecondWorkspace(FIXTURE_KEY.regular, JOURNEY_WORKSPACE)).id;
    test.skip(!(await slackEntry(workspaceId)).configured, "this deployment has no slack-app bag; the unconfigured journey runs instead");
    await ensureConnectorHandle(FIXTURE_KEY.regular, workspaceId, SLACK_HANDLE);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, "integrations"));
    const row = page.getByTestId("integration-slack");
    // Slack labels its connection with the team it landed on.
    await expect(row).toContainText(`Connected · ${SLACK_TEAM_NAME}`);

    await row.getByRole("button", { name: `Manage ${SLACK}`, exact: true }).click();
    const dialog = page.getByRole("dialog", { name: SLACK });
    await expect(dialog).toContainText(`Installed on ${SLACK_TEAM_NAME}.`);
    await expect(dialog).toContainText("Channel mentions and bot replies.");
    await dialog.getByRole("button", { name: `Disconnect ${SLACK}`, exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });
    await expect(row).toContainText("Not connected");
    const stored = await clientFor(FIXTURE_KEY.regular).get<{ secrets: { name: string }[] }>(`/v1/workspaces/${workspaceId}/secrets`);
    expect(stored.secrets.map(secret => secret.name)).not.toContain(SLACK_HANDLE.name);

    await page.route(SLACK_CONSENT, route => route.fulfill({ status: 200, contentType: "text/html", body: "<title>Slack consent</title>" }));
    await row.getByRole("button", { name: `Connect ${SLACK}`, exact: true }).click();
    await page.waitForURL(/^https:\/\/slack\.com\/oauth\/v2\/authorize\?/, { timeout: ACTION_TIMEOUT_MS });
    const callback = new URL(new URL(page.url()).searchParams.get("redirect_uri") ?? "", "https://invalid.example");
    expect(callback.origin, "consent must return to this dashboard").toBe(new URL(baseURL ?? "").origin);
    expect(callback.pathname).toMatch(/\/connectors\/slack\/callback$/);
  });
});
