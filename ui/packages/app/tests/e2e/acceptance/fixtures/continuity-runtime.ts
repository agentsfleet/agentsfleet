import { expect, type Page } from "@playwright/test";
import { z } from "zod";
import { steerMessagesUrl } from "@/lib/api/fleets-types";
import { REPLY_ID_SUFFIX } from "@/components/domain/useFleetThreadEntries";
import { type ApiClient } from "./api-client";
import { runtimeHistory, threadPage, type ContinuityEvent } from "./continuity-history";
import { CONTINUITY, runtimeMessage } from "./continuity-scenario";
import { visibleReply } from "./continuity-visible";

const TERMINAL = /^(processed|fleet_error|gate_blocked)$/;
const POLL_INTERVALS = [500, 1_000, 2_000];
const PROCESSED = "processed";
const SetupReceipt = z.object({kind: z.enum(["setup_saved", "setup_ready"]), revision: z.string()}).strict();
const HTTP_ACCEPTED = 202;
const AcceptedReceipt = z.object({status: z.literal("accepted"), event_id: z.string(), replayed: z.boolean()});
type ExpectedTurn = {id: string; request: string; answer: string};

export class ContinuityRuntime {
  private readonly turns: ExpectedTurn[] = [];

  constructor(private readonly page: Page, private readonly client: ApiClient,
    private readonly workspace: string, private readonly fleetId: string) {}

  get scope(): string { return `/v1/workspaces/${this.workspace}/fleets/${this.fleetId}`; }

  async converse(count: number, revision: number, name: string): Promise<void> {
    for (let index = 0; index < count; index++) {
      await this.send(name);
      console.log(JSON.stringify({fleet: name, runtime: revision, turn: this.turns.length, status: "PROCESSED"}));
    }
    await this.assertHistory(revision);
  }

  private async send(name: string): Promise<void> {
    const request = `For project ${name}: ${runtimeMessage(this.turns.length + 1)}`;
    const accepted = this.page.waitForResponse(response => response.request().method() === "POST" &&
      new URL(response.url()).pathname === steerMessagesUrl(this.workspace, this.fleetId));
    await this.page.getByRole("textbox", {name: "Message your fleet…", exact: true}).fill(request);
    await this.page.getByRole("button", {name: "Send", exact: true}).click();
    const response = await accepted;
    expect(response.status(), "fleet accepted this message").toBe(HTTP_ACCEPTED);
    const receipt = AcceptedReceipt.parse(await response.json());
    expect(receipt.replayed, "this message is a new send").toBe(false);
    let event: ContinuityEvent | undefined;
    await expect.poll(async () => {
      const rows = (await threadPage(this.client, this.scope)).items;
      event = rows.find(row => row.event_id === receipt.event_id);
      return event?.status ?? "absent";
    }, {timeout: CONTINUITY.turnTimeoutMs, intervals: POLL_INTERVALS}).toMatch(TERMINAL);
    expect(event?.status, `runtime event ${receipt.event_id}: ${event?.failure_label ?? ""}`).toBe(PROCESSED);
    expect(event!.fleet_id).toBe(this.fleetId);
    expect(event!.workspace_id).toBe(this.workspace);
    expect(event!.response_text?.trim().length, "a processed reply has text").toBeGreaterThan(0);
    const reply = this.page.locator(`[data-message-id="${receipt.event_id}${REPLY_ID_SUFFIX}"] [data-role="assistant"]`);
    await expect(reply).toBeVisible({timeout: CONTINUITY.turnTimeoutMs});
    await expect(reply).toContainText(await visibleReply(this.page, event!.response_text!), {timeout: CONTINUITY.turnTimeoutMs});
    await expect(reply.getByTestId("fleet-reply-figures")).toBeVisible();
    this.turns.push({id: event!.event_id, request, answer: event!.response_text!});
  }

  async assertHistory(revision: number): Promise<void> {
    const history = await runtimeHistory(this.client, this.scope);
    const receipts = history.filter(row => row.event_type === "activity");
    const expected = Array.from({length: revision}, (_,index) => [
      {kind: "setup_saved", revision: String(index + 1)},
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
      expect(row?.status).toBe(PROCESSED);
      expect(z.object({message: z.string()}).parse(JSON.parse(row!.request_json)).message).toBe(turn.request);
      expect(row?.response_text).toBe(turn.answer);
      expect(row?.fleet_id).toBe(this.fleetId);
      expect(row?.workspace_id).toBe(this.workspace);
    }
  }
}
