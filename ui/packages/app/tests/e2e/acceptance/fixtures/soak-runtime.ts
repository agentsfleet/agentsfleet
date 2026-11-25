import { expect, type Page } from "@playwright/test";
import { z } from "zod";
import { steerMessagesUrl } from "@/lib/api/fleets-types";
import { REPLY_ID_SUFFIX } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { type ApiClient } from "./api-client";
import { PINNED_CATEGORY } from "@/lib/api/fleet-client";
import { runtimeHistory, threadPage, type SoakTurn } from "./soak-history";
import { SOAK, pinMessage, runtimeMessage } from "./soak-scenario";
import { expandIfFolded, visibleReply } from "./soak-visible";
import { MAX_CONSECUTIVE_FAILURES, captureFailure, classify, outageLedger, outageWatch, pendingRequests } from "./soak-outage";

const TERMINAL = /^(processed|fleet_error|gate_blocked)$/;
const POLL_INTERVALS = [500, 1_000, 2_000];
const PROCESSED = "processed";
const HTTP_ACCEPTED = 202;
const MemoryPage = z.object({items: z.array(z.object({key: z.string(), content: z.string(), category: z.string(),
  writer_fleet_id: z.string()})), next_cursor: z.string().nullable()});
const AcceptedReceipt = z.object({status: z.literal("accepted"), turn_id: z.string(), replayed: z.boolean()});
type ExpectedTurn = {id: string; request: string; answer: string};

export class SoakRuntime {
  private turns: ExpectedTurn[] = [];
  private pendingInput = "";
  private pendingTurn: string | null = null;
  /** Messages sent so far, including outage turns; numbers the next question. */
  private sentCount = 0;
  /** Turns that failed during a recorded outage; they were never resent. */
  readonly outageTurns = new Set<string>();
  onTurn: (next: number) => Promise<void> = async () => {};

  constructor(private readonly page: Page, private readonly client: ApiClient,
    private readonly workspace: string, private readonly fleetId: string) {}

  get scope(): string { return `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`; }

  /** Continue from the fleet's durable history, as a resumed journey does. */
  async hydrate(): Promise<void> {
    const rows = (await runtimeHistory(this.client, this.scope)).filter(row => row.event_type !== "activity");
    this.turns = rows.map(row => ({id: row.turn_id,
      request: z.object({message: z.string()}).parse(JSON.parse(row.request_json)).message,
      answer: row.response_text ?? ""}));
    this.sentCount = this.turns.length;
  }

  /** `step` labels the journey step in logs and the outage ledger. */
  async converse(count: number, step: number, name: string, from = 0): Promise<void> {
    let consecutive = 0;
    for (let index = from; index < count; index++) {
      // Checkpoint past this turn before sending it: a crash mid-send resumes at the next turn, never a resend.
      await this.onTurn(index + 1);
      const started = Date.now();
      const at = new Date(started).toISOString();
      try {
        await this.send(this.question(name));
      } catch (error) {
        const failedAt = Date.now();
        const outage = outageWatch.during(started, failedAt);
        const record = {fleet: name, stage: "runtime", step, step_turn: index + 1,
          turn: this.sentCount, input: this.pendingInput, turn_id: this.pendingTurn, sentAt: at,
          failedAt: new Date(failedAt).toISOString(), workspace: this.workspace, fleetId: this.fleetId,
          url: this.page.url(), error: String(error),
          classification: classify(outage),
          outage: outage ? {kind: outage.kind, failedProbes: outage.failedProbes.length} : null};
        console.error(JSON.stringify({soakFailure: record}));
        consecutive += 1;
        if (consecutive >= MAX_CONSECUTIVE_FAILURES) throw error;
        // A failed turn stays failed and is never resent; the next turn continues the same fleet.
        Object.assign(record, {pending: pendingRequests(this.page), screenshot: await captureFailure(this.page, `${name}-step${step}-turn${index + 1}`)});
        outageLedger.push(record);
        if (outage) await outageWatch.waitHealthy();
        await this.page.reload();
        const sent = this.sentCount;
        await this.hydrate();
        const stored = this.turns.find(turn => turn.request === this.pendingInput);
        if (stored) this.outageTurns.add(stored.id);
        const row = stored ? (await runtimeHistory(this.client, this.scope)).find(item => item.turn_id === stored.id) : undefined;
        Object.assign(record, {stored: Boolean(stored), turn_id: stored?.id ?? this.pendingTurn,
          turnStatus: row?.status ?? null, failureLabel: row?.failure_label ?? null});
        console.error(JSON.stringify({soakFailureSettled: record}));
        this.sentCount = sent;
        await this.onTurn(index + 1);
        continue;
      }
      consecutive = 0;
      console.log(JSON.stringify({fleet: name, step, turn: this.turns.length, status: "PROCESSED"}));
      await this.onTurn(index + 1);
    }
    await this.assertHistory();
  }

  /** Asks the fleet to pin one rule; the rule is read back from its memory, never from the reply. */
  async pin(rule: string): Promise<void> {
    await this.send(pinMessage(rule));
  }

  /** Every rule pinned so far is one of the fleet's own `core` entries, byte for byte. */
  async assertPinned(rules: readonly string[]): Promise<void> {
    const pinned: string[] = [];
    let cursor: string | null = null;
    for (let page = 0; page < SOAK.maxHistoryPages; page++) {
      const query = new URLSearchParams({limit: String(SOAK.pageLimit)});
      if (cursor) query.set("starting_after", cursor);
      const result = MemoryPage.parse(await this.client.get<unknown>(`${this.scope}/memories?${query}`));
      pinned.push(...result.items.filter(item => item.category === PINNED_CATEGORY && item.writer_fleet_id === this.fleetId)
        .map(item => item.content));
      cursor = result.next_cursor;
      if (!cursor) break;
    }
    for (const rule of rules) expect(pinned.some(content => content.includes(rule)), `the fleet pinned ${rule}`).toBe(true);
  }

  private question(name: string): string {
    return `For project ${name}: ${runtimeMessage(this.sentCount + 1)}`;
  }

  private async send(request: string): Promise<void> {
    this.sentCount += 1;
    this.pendingInput = request;
    this.pendingTurn = null;
    const accepted = this.page.waitForResponse(response => response.request().method() === "POST" &&
      new URL(response.url()).pathname === steerMessagesUrl(this.workspace, this.fleetId));
    await this.page.getByRole("textbox", {name: "Message your fleet…", exact: true}).fill(request);
    await this.page.getByRole("button", {name: "Send", exact: true}).click();
    const response = await accepted;
    expect(response.status(), "fleet accepted this message").toBe(HTTP_ACCEPTED);
    const receipt = AcceptedReceipt.parse(await response.json());
    this.pendingTurn = receipt.turn_id;
    // The dashboard retries an unconfirmed send under the same identity, and the
    // API answers that retry `replayed`; a new message is one whose turn is new.
    expect(this.turns.some(turn => turn.id === receipt.turn_id), "this message has its own turn").toBe(false);
    let saved: SoakTurn | undefined;
    await expect.poll(async () => {
      const rows = (await threadPage(this.client, this.scope)).items;
      saved = rows.find(row => row.turn_id === receipt.turn_id);
      return saved?.status ?? "absent";
    }, {timeout: SOAK.turnTimeoutMs, intervals: POLL_INTERVALS}).toMatch(TERMINAL);
    expect(saved?.status, `runtime turn ${receipt.turn_id}: ${saved?.failure_label ?? ""}`).toBe(PROCESSED);
    expect(saved!.fleet_id).toBe(this.fleetId);
    expect(saved!.workspace_id).toBe(this.workspace);
    expect(saved!.response_text?.trim().length, "a processed reply has text").toBeGreaterThan(0);
    const reply = this.page.locator(`[data-message-id="${receipt.turn_id}${REPLY_ID_SUFFIX}"] [data-role="assistant"]`);
    await expect(reply).toBeVisible({timeout: SOAK.turnTimeoutMs});
    await expandIfFolded(this.page.locator(`[data-message-id="${receipt.turn_id}${REPLY_ID_SUFFIX}"]`));
    await expect(reply).toContainText(await visibleReply(this.page, saved!.response_text!), {timeout: SOAK.turnTimeoutMs});
    await expect(reply.getByRole("button", {name: "Copy reply", exact: true})).toBeVisible();
    this.turns.push({id: saved!.turn_id, request, answer: saved!.response_text!});
  }

  async assertHistory(): Promise<void> {
    const history = await runtimeHistory(this.client, this.scope);
    const rows = history.filter(row => row.event_type !== "activity");
    expect(rows).toHaveLength(this.turns.length);
    expect(new Set(rows.map(row => row.turn_id)).size).toBe(this.turns.length);
    for (const turn of this.turns) {
      const row = rows.find(item => item.turn_id === turn.id);
      expect(row?.event_type).toBe("chat");
      expect(z.object({message: z.string()}).parse(JSON.parse(row!.request_json)).message).toBe(turn.request);
      // An outage turn is reported from the ledger; only its single stored input is required.
      if (this.outageTurns.has(turn.id)) continue;
      expect(row?.status).toBe(PROCESSED);
      expect(z.object({message: z.string()}).parse(JSON.parse(row!.request_json)).message).toBe(turn.request);
      expect(row?.response_text).toBe(turn.answer);
      expect(row?.fleet_id).toBe(this.fleetId);
      expect(row?.workspace_id).toBe(this.workspace);
    }
  }
}
