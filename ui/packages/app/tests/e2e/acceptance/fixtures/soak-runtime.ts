import { expect, type Page } from "@playwright/test";
import { z } from "zod";
import { steerMessagesUrl } from "@/lib/api/fleets-types";
import { REPLY_ID_SUFFIX } from "@/components/domain/useFleetThreadEntries";
import { type ApiClient } from "./api-client";
import { runtimeHistory, threadPage, type SoakEvent } from "./soak-history";
import { SOAK, runtimeMessage } from "./soak-scenario";
import { expandIfFolded, visibleReply } from "./soak-visible";
import { MAX_CONSECUTIVE_FAILURES, captureFailure, classify, outageLedger, outageWatch, pendingRequests } from "./soak-outage";

const TERMINAL = /^(processed|fleet_error|gate_blocked)$/;
const POLL_INTERVALS = [500, 1_000, 2_000];
const PROCESSED = "processed";
// Mirrors lib/events/fleet-activity.ts: a saved receipt names where the save came from.
const SetupReceipt = z.object({kind: z.enum(["setup_saved", "setup_ready"]), revision: z.string(),
  origin: z.enum(["cli", "dashboard", "chat"]).optional()}).strict();
const BROWSER_ORIGIN = "dashboard";
const HTTP_ACCEPTED = 202;
const AcceptedReceipt = z.object({status: z.literal("accepted"), event_id: z.string(), replayed: z.boolean()});
type ExpectedTurn = {id: string; request: string; answer: string};

export class SoakRuntime {
  private turns: ExpectedTurn[] = [];
  private pendingInput = "";
  private pendingEvent: string | null = null;
  /** Messages sent so far, including outage turns; numbers the next question. */
  private sentCount = 0;
  /** Events whose turn failed during a recorded outage; they were never resent. */
  readonly outageEvents = new Set<string>();
  onTurn: (next: number) => Promise<void> = async () => {};

  constructor(private readonly page: Page, private readonly client: ApiClient,
    private readonly workspace: string, private readonly fleetId: string) {}

  get scope(): string { return `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`; }

  /** Continue from the fleet's durable history, as a resumed journey does. */
  async hydrate(): Promise<void> {
    const rows = (await runtimeHistory(this.client, this.scope)).filter(row => row.event_type !== "activity");
    this.turns = rows.map(row => ({id: row.event_id,
      request: z.object({message: z.string()}).parse(JSON.parse(row.request_json)).message,
      answer: row.response_text ?? ""}));
    this.sentCount = this.turns.length;
  }

  async converse(count: number, revision: number, name: string, from = 0): Promise<void> {
    let consecutive = 0;
    for (let index = from; index < count; index++) {
      // Checkpoint past this turn before sending it: a crash mid-send resumes at the next turn, never a resend.
      await this.onTurn(index + 1);
      const started = Date.now();
      const at = new Date(started).toISOString();
      try {
        await this.send(name);
      } catch (error) {
        const failedAt = Date.now();
        const outage = outageWatch.during(started, failedAt);
        const record = {fleet: name, stage: "runtime", setup: revision, step_turn: index + 1,
          turn: this.sentCount, input: this.pendingInput, event: this.pendingEvent, sentAt: at,
          failedAt: new Date(failedAt).toISOString(), workspace: this.workspace, fleetId: this.fleetId,
          url: this.page.url(), error: String(error),
          classification: classify(outage),
          outage: outage ? {kind: outage.kind, failedProbes: outage.failedProbes.length} : null};
        console.error(JSON.stringify({soakFailure: record}));
        consecutive += 1;
        if (consecutive >= MAX_CONSECUTIVE_FAILURES) throw error;
        // A failed turn stays failed and is never resent; the next turn continues the same fleet.
        Object.assign(record, {pending: pendingRequests(this.page), screenshot: await captureFailure(this.page, `${name}-runtime${revision}-turn${index + 1}`)});
        outageLedger.push(record);
        if (outage) await outageWatch.waitHealthy();
        await this.page.reload();
        const sent = this.sentCount;
        await this.hydrate();
        const stored = this.turns.find(turn => turn.request === this.pendingInput);
        if (stored) this.outageEvents.add(stored.id);
        const row = stored ? (await runtimeHistory(this.client, this.scope)).find(item => item.event_id === stored.id) : undefined;
        Object.assign(record, {stored: Boolean(stored), event: stored?.id ?? this.pendingEvent,
          eventStatus: row?.status ?? null, failureLabel: row?.failure_label ?? null});
        console.error(JSON.stringify({soakFailureSettled: record}));
        this.sentCount = sent;
        await this.onTurn(index + 1);
        continue;
      }
      consecutive = 0;
      console.log(JSON.stringify({fleet: name, runtime: revision, turn: this.turns.length, status: "PROCESSED"}));
      await this.onTurn(index + 1);
    }
    await this.assertHistory(revision);
  }

  private async send(name: string): Promise<void> {
    this.sentCount += 1;
    const request = `For project ${name}: ${runtimeMessage(this.sentCount)}`;
    this.pendingInput = request;
    this.pendingEvent = null;
    const accepted = this.page.waitForResponse(response => response.request().method() === "POST" &&
      new URL(response.url()).pathname === steerMessagesUrl(this.workspace, this.fleetId));
    await this.page.getByRole("textbox", {name: "Message your fleet…", exact: true}).fill(request);
    await this.page.getByRole("button", {name: "Send", exact: true}).click();
    const response = await accepted;
    expect(response.status(), "fleet accepted this message").toBe(HTTP_ACCEPTED);
    const receipt = AcceptedReceipt.parse(await response.json());
    this.pendingEvent = receipt.event_id;
    // The dashboard retries an unconfirmed send under the same identity, and the
    // API answers that retry `replayed`; a new message is one whose event is new.
    expect(this.turns.some(turn => turn.id === receipt.event_id), "this message has its own event").toBe(false);
    let event: SoakEvent | undefined;
    await expect.poll(async () => {
      const rows = (await threadPage(this.client, this.scope)).items;
      event = rows.find(row => row.event_id === receipt.event_id);
      return event?.status ?? "absent";
    }, {timeout: SOAK.turnTimeoutMs, intervals: POLL_INTERVALS}).toMatch(TERMINAL);
    expect(event?.status, `runtime event ${receipt.event_id}: ${event?.failure_label ?? ""}`).toBe(PROCESSED);
    expect(event!.fleet_id).toBe(this.fleetId);
    expect(event!.workspace_id).toBe(this.workspace);
    expect(event!.response_text?.trim().length, "a processed reply has text").toBeGreaterThan(0);
    const reply = this.page.locator(`[data-message-id="${receipt.event_id}${REPLY_ID_SUFFIX}"] [data-role="assistant"]`);
    await expect(reply).toBeVisible({timeout: SOAK.turnTimeoutMs});
    await expandIfFolded(this.page.locator(`[data-message-id="${receipt.event_id}${REPLY_ID_SUFFIX}"]`));
    await expect(reply).toContainText(await visibleReply(this.page, event!.response_text!), {timeout: SOAK.turnTimeoutMs});
    await expect(reply.getByTestId("fleet-reply-figures")).toBeVisible();
    this.turns.push({id: event!.event_id, request, answer: event!.response_text!});
  }

  async assertHistory(revision: number): Promise<void> {
    const history = await runtimeHistory(this.client, this.scope);
    const receipts = history.filter(row => row.event_type === "activity");
    const expected = Array.from({length: revision}, (_,index) => [
      // Every save in this journey is made in the browser.
      {kind: "setup_saved", revision: String(index + 1), origin: BROWSER_ORIGIN},
      {kind: "setup_ready", revision: String(index + 1)},
    ]).flat();
    expect(receipts).toHaveLength(expected.length);
    expect(receipts.map(row => {
      expect(row.fleet_id).toBe(this.fleetId);
      expect(row.workspace_id).toBe(this.workspace);
      expect(row.actor).toBe("system");
      expect(row.status).toBe("recorded");
      expect(row.response_text).toBeNull();
      return SetupReceipt.parse(JSON.parse(row.request_json));
    })).toEqual(expect.arrayContaining(expected));
    const rows = history.filter(row => row.event_type !== "activity");
    expect(rows).toHaveLength(this.turns.length);
    expect(new Set(rows.map(row => row.event_id)).size).toBe(this.turns.length);
    for (const turn of this.turns) {
      const row = rows.find(item => item.event_id === turn.id);
      expect(row?.event_type).toBe("chat");
      expect(z.object({message: z.string()}).parse(JSON.parse(row!.request_json)).message).toBe(turn.request);
      // An outage turn is reported from the ledger; only its single stored input is required.
      if (this.outageEvents.has(turn.id)) continue;
      expect(row?.status).toBe(PROCESSED);
      expect(z.object({message: z.string()}).parse(JSON.parse(row!.request_json)).message).toBe(turn.request);
      expect(row?.response_text).toBe(turn.answer);
      expect(row?.fleet_id).toBe(this.fleetId);
      expect(row?.workspace_id).toBe(this.workspace);
    }
  }
}
