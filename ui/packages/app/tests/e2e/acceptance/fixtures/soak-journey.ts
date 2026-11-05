import { expect, type Page } from "@playwright/test";
import { FleetSchema, type Definition } from "@/lib/api/draft-types";
import { clientFor, type ClientHandle } from "./api-client";
import { SoakSetup } from "./soak-setup";
import { SoakRuntime } from "./soak-runtime";
import { draftHistory, runtimeHistory } from "./soak-history";
import { SOAK, SOAK_KIND, SOAK_STEPS, UNICODE_SAVED } from "./soak-scenario";
import { CHAT_COPY, NEW_FLEET, draftHref, editSetupLink, openSavedFleet } from "./private-chat";
import { writeFile } from "node:fs/promises";
import { trackRequests } from "./soak-outage";
import { workspaceHref } from "./nav";
import { purgeFixtureFleet } from "./teardown";

/** Where a journey stands: step -1 is the first setup chat; turn is the next turn to send. */
export type SoakManifest = {workspace: string; name: string; marker: string; fleet: string | null;
  drafts: string[]; revision: number; step: number; turn: number};
const AUTHOR_STEP = -1;

export class SoakJourney {
  readonly drafts: SoakSetup[] = [];
  fleetId = "";
  private revision = 1;
  private definition: Definition | undefined;
  private runtime: SoakRuntime | undefined;
  private readonly client;

  private step = AUTHOR_STEP;

  constructor(private readonly page: Page, private readonly handle: ClientHandle,
    private readonly workspace: string, readonly name: string, readonly marker: string,
    private readonly checkpointPath?: string) {
    this.client = clientFor(handle);
    trackRequests(page);
  }

  /** Record the next turn to send, so an interrupted run resumes there and never resends. */
  private async checkpoint(step: number, turn: number): Promise<void> {
    this.step = step;
    if (!this.checkpointPath) return;
    const manifest: SoakManifest = {workspace: this.workspace, name: this.name, marker: this.marker,
      fleet: this.fleetId || null, drafts: this.drafts.map(draft => draft.draftId).filter(Boolean),
      revision: this.revision, step, turn};
    await writeFile(this.checkpointPath, JSON.stringify(manifest, null, 1));
  }

  private watch(chat: SoakSetup | SoakRuntime): void {
    chat.onTurn = async next => this.checkpoint(this.step, next);
  }

  /** Rebuild every record a previous process created, from durable state only. */
  private async restore(from: SoakManifest): Promise<void> {
    expect(from.name, "the resumed manifest names this journey").toBe(this.name);
    for (const id of from.drafts) {
      const chat = new SoakSetup(this.page, this.client, this.workspace, this.page.locator("main"));
      await chat.hydrate(id);
      this.drafts.push(chat);
    }
    this.revision = from.revision;
    if (from.fleet) {
      this.fleetId = from.fleet;
      this.definition = FleetSchema.parse(await this.client.get<unknown>(
        `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`)).definition;
      this.runtime = new SoakRuntime(this.page, this.client, this.workspace, this.fleetId);
      this.watch(this.runtime);
      await this.runtime.hydrate();
    }
    console.log(JSON.stringify({soakResumed: from}));
  }

  async run(from?: SoakManifest): Promise<void> {
    if (from) await this.restore(from);
    const start = from ? {step: from.step, turn: from.turn} : {step: AUTHOR_STEP, turn: 0};
    if (start.step === AUTHOR_STEP) await this.author(start.turn);
    if (start.step >= 0 && !this.fleetId) throw new Error(`${this.name} has no saved fleet; later steps cannot run`);
    for (let index = Math.max(start.step, 0); index < SOAK_STEPS.length; index++) {
      const step = SOAK_STEPS[index]!;
      const turn = index === start.step ? start.turn : 0;
      await this.checkpoint(index, turn);
      if (step.kind === SOAK_KIND.runtime) {
        if (turn > 0) await this.page.goto(workspaceHref(this.workspace, `fleets/${this.fleetId}`));
        await this.runtime!.converse(step.count, this.revision, this.name, turn);
      } else if (step.kind === SOAK_KIND.reopen) await this.reopen();
      else await this.edit(step.revision, turn);
    }
    await this.checkpoint(SOAK_STEPS.length, 0);
    await this.page.reload();
    await this.runtime!.assertHistory(this.revision);
    for (const draft of this.drafts) await draft.assertHistory();
    await this.assertSaved(4);
    await this.assertUnicodeSaved();
  }

  /** Read from the saved definition itself, never from a model reply. */
  private async assertUnicodeSaved(): Promise<void> {
    const fleet = FleetSchema.parse(await this.client.get<unknown>(
      `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`));
    const instructions = fleet.definition?.instructions_md ?? "";
    for (const rule of UNICODE_SAVED) expect(instructions, `saved setup keeps ${rule}`).toContain(rule);
    expect(instructions, "the replaced text is gone").not.toContain("東京");
  }

  private async author(from: number): Promise<void> {
    await this.checkpoint(AUTHOR_STEP, from);
    let chat: SoakSetup;
    // No stored draft means no stored turn: the chat starts fresh without resending anything.
    if (from > 0 && this.drafts.length > 0) {
      chat = this.drafts.at(-1)!;
      await this.page.goto(draftHref(this.workspace, chat.draftId));
    } else {
      await this.page.goto(workspaceHref(this.workspace, NEW_FLEET));
      chat = new SoakSetup(this.page, this.client, this.workspace, this.page.locator("main"));
      this.drafts.push(chat);
      // Without a stored draft the creating message never landed, so the chat begins at its first turn.
      from = 0;
    }
    this.watch(chat);
    await chat.converse(SOAK.authoredMessages, 1, this.name, from);
    this.definition = chat.definition;
    await this.page.getByRole("button", {name: CHAT_COPY.save, exact: true}).click();
    this.fleetId = FleetSchema.shape.id.parse(await openSavedFleet(this.page));
    console.log(JSON.stringify({soakIdentifiers: this.identifiers()}));
    this.runtime = new SoakRuntime(this.page, this.client, this.workspace, this.fleetId);
    this.watch(this.runtime);
    await this.assertSaved(1);
  }

  private async edit(revision: number, from: number): Promise<void> {
    let chat: SoakSetup;
    // The author chat plus one per earlier edit precede this one; fewer means it was never created.
    if (from > 0 && this.drafts.length >= revision) {
      chat = this.drafts.at(-1)!;
      await this.page.goto(draftHref(this.workspace, chat.draftId));
    } else {
      if (from > 0) await this.page.goto(workspaceHref(this.workspace, `fleets/${this.fleetId}`));
      await editSetupLink(this.page).click();
      chat = new SoakSetup(this.page, this.client, this.workspace, this.page.locator("main"), this.definition);
      this.drafts.push(chat);
    }
    await expect(this.page.getByLabel(CHAT_COPY.composer)).toBeVisible();
    this.watch(chat);
    await chat.converse(SOAK.setupMessages, revision, this.name, from);
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
