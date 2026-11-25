import { expect, type Page } from "@playwright/test";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { clientFor, type ClientHandle } from "./api-client";
import { SoakRuntime } from "./soak-runtime";
import { runtimeHistory } from "./soak-history";
import { PINNED_RULES, SOAK_KIND, SOAK_STEPS, firstMessage } from "./soak-scenario";
import { writeFile } from "node:fs/promises";
import { APPLICATION_FAILURE, outageLedger, trackRequests } from "./soak-outage";
import { awaitFirstTurnSettled, createFixtureFleet } from "./fleet-create";
import { fleetNavigation, openFleetFromSidebar, workspaceHref } from "./nav";
import { purgeFixtureFleet } from "./teardown";

/** Where a journey stands: step -1 is the fleet's first message; turn is the next turn to send. */
export type SoakManifest = {workspace: string; name: string; marker: string; fleet: string | null;
  pins: number; step: number; turn: number};
const CREATE_STEP = -1;

export class SoakJourney {
  fleetId = "";
  private pins = 0;
  private runtime: SoakRuntime | undefined;
  private readonly client;

  private step = CREATE_STEP;

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
      fleet: this.fleetId || null, pins: this.pins, step, turn};
    await writeFile(this.checkpointPath, JSON.stringify(manifest, null, 1));
  }

  private watch(chat: SoakRuntime): void {
    chat.onTurn = async next => this.checkpoint(this.step, next);
  }

  /** Rebuild every record a previous process created, from durable state only. */
  private async restore(from: SoakManifest): Promise<void> {
    expect(from.name, "the resumed manifest names this journey").toBe(this.name);
    this.pins = from.pins;
    if (from.fleet) {
      this.fleetId = from.fleet;
      this.runtime = new SoakRuntime(this.page, this.client, this.workspace, this.fleetId);
      this.watch(this.runtime);
      await this.runtime.hydrate();
    }
    console.log(JSON.stringify({soakResumed: from}));
  }

  async run(from?: SoakManifest): Promise<void> {
    if (from) await this.restore(from);
    const start = from ? {step: from.step, turn: from.turn} : {step: CREATE_STEP, turn: 0};
    if (start.step === CREATE_STEP && !this.fleetId) await this.create();
    if (!this.fleetId) throw new Error(`${this.name} has no fleet; later steps cannot run`);
    await this.page.goto(workspaceHref(this.workspace, `fleets/${this.fleetId}`));
    for (let index = Math.max(start.step, 0); index < SOAK_STEPS.length; index++) {
      const step = SOAK_STEPS[index]!;
      const turn = index === start.step ? start.turn : 0;
      await this.checkpoint(index, turn);
      if (step.kind === SOAK_KIND.runtime) {
        if (turn > 0) await this.page.goto(workspaceHref(this.workspace, `fleets/${this.fleetId}`));
        await this.runtime!.converse(step.count, index, this.name, turn);
      } else if (step.kind === SOAK_KIND.reopen) await this.reopen();
      else await this.pin(step.rule, turn);
    }
    await this.checkpoint(SOAK_STEPS.length, 0);
    await this.page.reload();
    await this.runtime!.assertHistory();
    await this.runtime!.assertPinned(PINNED_RULES.slice(0, this.pins));
    await this.assertFleet();
    // Failed turns were moved past so the run could finish; only outages are excused.
    const failed = outageLedger.filter(entry => entry.fleet === this.name && entry.classification === APPLICATION_FAILURE);
    expect(failed, `${this.name} recorded failed turns that no outage explains`).toEqual([]);
  }

  /** The fleet and its first turn, made by one message; the chat starts once that turn has its answer. */
  private async create(): Promise<void> {
    await this.checkpoint(CREATE_STEP, 0);
    const {fleet, turnId} = await createFixtureFleet(this.handle, this.workspace, {name: this.name, message: firstMessage(this.name)});
    expect(fleet.name, "the soak fleet keeps its own name").toBe(this.name);
    this.fleetId = fleet.id;
    await awaitFirstTurnSettled(this.handle, this.workspace, fleet.id, turnId);
    console.log(JSON.stringify({soakIdentifiers: this.identifiers()}));
    this.runtime = new SoakRuntime(this.page, this.client, this.workspace, this.fleetId);
    this.watch(this.runtime);
    await this.runtime.hydrate();
    await this.assertFleet();
  }

  /** One pin is one chat message; a resumed run past it does not ask twice. */
  private async pin(rule: number, from: number): Promise<void> {
    if (from === 0) {
      await this.runtime!.pin(PINNED_RULES[rule]!);
      await this.checkpoint(this.step, 1);
    }
    this.pins = rule + 1;
    await this.runtime!.assertPinned(PINNED_RULES.slice(0, this.pins));
  }

  private async reopen(): Promise<void> {
    const history = await fleetNavigation(this.page);
    const conversation = history.locator(`a[href="${workspaceHref(this.workspace, `fleets/${this.fleetId}`)}"]`);
    await expect(conversation).toHaveCount(1);
    await expect(conversation.getByText(this.name, {exact: true})).toBeVisible();
    await openFleetFromSidebar(this.page, this.workspace, this.fleetId);
    await expect(this.page).toHaveURL(new RegExp(`/fleets/${this.fleetId}$`));
    await this.assertFleet();
    await this.runtime!.assertHistory();
  }

  private async assertFleet(): Promise<void> {
    const fleet = FleetDetailSchema.parse(await this.client.get<unknown>(
      `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`));
    expect(fleet.name).toBe(this.name);
    expect(fleet.status).toBe("active");
  }

  async snapshot() {
    const scope = `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`;
    return {name: this.name, marker: this.marker,
      fleet: this.fleetId ? FleetDetailSchema.parse(await this.client.get<unknown>(scope)) : null,
      messages: this.fleetId ? await runtimeHistory(this.client, scope) : [],
    };
  }

  identifiers() {
    return {workspace: this.workspace, name: this.name, fleet: this.fleetId || null};
  }

  async assertIsolated(otherMarkers: readonly string[]): Promise<void> {
    const evidence = JSON.stringify(await this.snapshot());
    for (const marker of otherMarkers) expect(evidence, `${this.name} excludes ${marker}`).not.toContain(marker);
  }

  async cleanup(): Promise<void> {
    if (this.fleetId) await purgeFixtureFleet(this.handle, this.workspace, this.fleetId);
  }
}
