/**
 * onboarding-chat-journey.spec.ts — a fleet is a chat from its first message.
 *
 * The path a new person takes, typed rather than seeded: open New fleet, type
 * a first message, land in that fleet's chat and read its answer. Then ask the
 * fleet for a workspace secret: it requests access, a card asks the person
 * once, Approve resumes the waiting run, and the run answers holding it.
 * Nothing is set up, reviewed or saved on the way.
 */
import { expect, test } from "@playwright/test";
import { z } from "zod";
import { GRANT_STATUS, GrantSchema } from "@/lib/api/fleet-detail";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { accessMessage, accessReply, probeMessage, probeReply } from "./fixtures/probe";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { expectSidebarFleetState } from "./fixtures/lifecycle";
import { uniqueTag } from "./fixtures/observation";
import { purgeFixtureFleet } from "./fixtures/teardown";

const JOURNEY_TIMEOUT_MS = 480_000;
// One provider round trip on a runner, with the queue in front of it.
const ANSWER_TIMEOUT_MS = 180_000;
const DECISION_TIMEOUT_MS = 15_000;
const CHAT_LABEL = "Fleet chat";
const COMPOSER_LABEL = "Chat composer";
// The secret only has to exist and name a host the egress allowlist reads; no run sends it anywhere.
const SECRET_HOST = "example.com";
const GrantsSchema = z.object({items: z.array(GrantSchema)});

test.describe("onboarding chat", () => {
  let workspaceId = "";
  let fleetId: string | null = null;
  let secretName: string | null = null;

  test.afterAll(async () => {
    if (workspaceId && fleetId) await purgeFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId);
    if (workspaceId && secretName) {
      await clientFor(FIXTURE_KEY.regular).delete(`/v1/workspaces/${workspaceId}/secrets/${encodeURIComponent(secretName)}`)
        .catch(() => undefined);
    }
  });

  test("a first message opens a fleet that answers, asks for a secret, and resumes holding it once approved", async ({ page }) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    // A fleet may name only letters, digits and `_`, so the secret uses that spelling.
    secretName = `onboarding_secret_${uniqueTag().replaceAll("-", "_")}`;
    await clientFor(FIXTURE_KEY.regular).post(`/v1/workspaces/${workspaceId}/secrets`,
      {name: secretName, data: {token: `fixture-${uniqueTag()}`, host: SECRET_HOST}});
    await signInAs(page, FIXTURE_KEY.regular);

    // One message on New fleet makes the fleet and its first turn, and the page becomes its chat.
    const hello = uniqueTag();
    fleetId = await createFleetViaUI(page, workspaceId, probeMessage(hello));
    const chat = page.getByLabel(CHAT_LABEL);
    await expect(chat.getByText(probeReply(hello)).first()).toBeVisible({ timeout: ANSWER_TIMEOUT_MS });

    // The fleet asks for the secret by tool; the run waits on a card instead of answering.
    const granted = uniqueTag();
    const composer = page.getByLabel(COMPOSER_LABEL, { exact: true });
    await composer.getByRole("textbox").fill(accessMessage(secretName, granted));
    await composer.getByRole("button", { name: "Send", exact: true }).click();
    const card = page.locator("#fleet-approvals [data-approval-gate]");
    await expect(card).toBeVisible({ timeout: ANSWER_TIMEOUT_MS });
    await expect(card.getByRole("definition").filter({ hasText: secretName })).toBeVisible();
    await expect(card.getByRole("button", { name: "Deny", exact: true })).toBeVisible();

    // One Approve: the grant stands and the waiting run resumes with it.
    await card.getByRole("button", { name: "Approve", exact: true }).click();
    await expect(card.getByText("Request approved.", { exact: true })).toBeVisible({ timeout: DECISION_TIMEOUT_MS });
    await expect(card.getByRole("button", { name: "Approve", exact: true })).toHaveCount(0);
    const grants = GrantsSchema.parse(await clientFor(FIXTURE_KEY.regular)
      .get<unknown>(`/v1/workspaces/${workspaceId}/fleets/${fleetId}/integration-grants`)).items;
    expect(grants.filter(grant => grant.name === secretName).map(grant => grant.status)).toEqual([GRANT_STATUS.approved]);
    await expect(chat.getByText(accessReply(granted)).first()).toBeVisible({ timeout: ANSWER_TIMEOUT_MS });

    // The fleet it made is in the sidebar, active.
    await expectSidebarFleetState(page, fleetId, "active");
  });
});
