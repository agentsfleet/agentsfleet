import { expect, type Page } from "@playwright/test";
import { FleetSchema, type Definition } from "@/lib/api/draft-types";
import { clientFor, type ClientHandle } from "./api-client";
import { ContinuitySetup } from "./continuity-setup";
import { ContinuityRuntime } from "./continuity-runtime";
import { draftHistory, runtimeHistory } from "./continuity-history";
import { CONTINUITY, CONTINUITY_KIND, CONTINUITY_STEPS } from "./continuity-scenario";
import { CHAT_COPY, NEW_FLEET, editSetupLink, openSavedFleet } from "./private-chat";
import { workspaceHref } from "./nav";
import { purgeFixtureFleet } from "./teardown";

export class ContinuityJourney {
  readonly drafts: ContinuitySetup[] = [];
  fleetId = "";
  private revision = 1;
  private definition: Definition | undefined;
  private runtime: ContinuityRuntime | undefined;
  private readonly client;

  constructor(private readonly page: Page, private readonly handle: ClientHandle,
    private readonly workspace: string, readonly name: string, readonly marker: string) {
    this.client = clientFor(handle);
  }

  async run(): Promise<void> {
    await this.author();
    for (const step of CONTINUITY_STEPS) {
      if (step.kind === CONTINUITY_KIND.runtime) await this.runtime!.converse(step.count, this.revision, this.name);
      else if (step.kind === CONTINUITY_KIND.reopen) await this.reopen();
      else await this.edit(step.revision);
    }
    await this.page.reload();
    await this.runtime!.assertHistory(this.revision);
    for (const draft of this.drafts) await draft.assertHistory();
    await this.assertSaved(4);
  }

  private async author(): Promise<void> {
    await this.page.goto(workspaceHref(this.workspace, NEW_FLEET));
    const chat = new ContinuitySetup(this.page, this.client, this.workspace, this.page.locator("main"));
    this.drafts.push(chat);
    await chat.converse(CONTINUITY.authoredMessages, 1, this.name);
    this.definition = chat.definition;
    await this.page.getByRole("button", {name: CHAT_COPY.save, exact: true}).click();
    this.fleetId = FleetSchema.shape.id.parse(await openSavedFleet(this.page));
    console.log(JSON.stringify({continuityIdentifiers: this.identifiers()}));
    this.runtime = new ContinuityRuntime(this.page, this.client, this.workspace, this.fleetId);
    await this.assertSaved(1);
  }

  private async edit(revision: number): Promise<void> {
    await editSetupLink(this.page).click();
    await expect(this.page.getByLabel(CHAT_COPY.composer)).toBeVisible();
    const chat = new ContinuitySetup(this.page, this.client, this.workspace, this.page.locator("main"));
    this.drafts.push(chat);
    await chat.converse(CONTINUITY.setupMessages, revision, this.name);
    this.definition = chat.definition;
    await this.page.getByRole("button", {name: CHAT_COPY.apply, exact: true}).click();
    expect(await openSavedFleet(this.page, String(revision))).toBe(this.fleetId);
    this.revision = revision;
    await this.assertSaved(revision);
  }

  private async reopen(): Promise<void> {
    await this.page.getByRole("link", {name: "Your fleets", exact: true}).click();
    await expect(this.page).toHaveURL(new RegExp(`/w/${this.workspace}/fleets$`));
    // The card's chosen name must lead back to this exact fleet, even with ten cards.
    const card = this.page.getByRole("article").filter({has: this.page.locator(
      `a[href="${workspaceHref(this.workspace, `fleets/${this.fleetId}`)}"]`)});
    await expect(card).toHaveCount(1);
    await expect(card.getByText(this.name, {exact: true})).toBeVisible();
    await card.getByRole("link").click();
    await expect(this.page).toHaveURL(new RegExp(`/fleets/${this.fleetId}$`));
    await this.assertSaved(this.revision);
    await this.runtime!.assertHistory(this.revision);
  }

  private async assertSaved(revision: number): Promise<void> {
    const fleet = FleetSchema.parse(await this.client.get<unknown>(
      `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`));
    expect(fleet.name).toBe(this.name);
    expect(fleet.setup_revision).toBe(String(revision));
    expect(this.definition, "the setup saved a validated candidate").toBeDefined();
    expect(fleet.definition, "the saved setup matches the candidate before and after navigation").toEqual(this.definition);
  }

  async snapshot() {
    const drafts = await Promise.all(this.drafts.filter(draft => draft.draftId).map(async draft => ({
      definition: await this.client.get<unknown>(draft.scope),
      messages: await draftHistory(this.client, draft.scope),
    })));
    const scope = `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`;
    return {name: this.name, marker: this.marker, drafts,
      fleet: this.fleetId ? FleetSchema.parse(await this.client.get<unknown>(scope)) : null,
      messages: this.fleetId ? await runtimeHistory(this.client, scope) : [],
    };
  }

  identifiers() {
    return {workspace: this.workspace, name: this.name, fleet: this.fleetId || null,
      drafts: this.drafts.map(draft => draft.draftId).filter(Boolean)};
  }

  async assertIsolated(otherMarkers: readonly string[]): Promise<void> {
    const evidence = JSON.stringify(await this.snapshot());
    for (const marker of otherMarkers) expect(evidence, `${this.name} excludes ${marker}`).not.toContain(marker);
  }

  async cleanup(): Promise<void> {
    if (this.fleetId) await purgeFixtureFleet(this.handle, this.workspace, this.fleetId);
    for (const draft of this.drafts) {
      if (draft.draftId) await this.client.delete(`/v1/workspaces/${this.workspace}/drafts/${draft.draftId}`);
    }
  }
}
