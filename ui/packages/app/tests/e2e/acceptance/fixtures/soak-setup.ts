import { expect, type Locator, type Page } from "@playwright/test";
import { DraftSchema, MESSAGE_ROLE, MESSAGE_STATE, type Definition } from "@/lib/api/draft-types";
import { type ApiClient } from "./api-client";
import { draftHistory } from "./soak-history";
import { SOAK, INSTRUCTION_CHECK, initialSetupMessage, setupMessage, type InstructionCheck, type SetupRequest } from "./soak-scenario";
import { CHAT_COPY, draftHref } from "./private-chat";
import { expandIfFolded, visibleReply } from "./soak-visible";
import { MAX_CONSECUTIVE_FAILURES, captureFailure, classify, outageLedger, outageWatch, pendingRequests } from "./soak-outage";

const TERMINAL = /^(completed|failed|timed_out|cancelled)$/;
const POLL_INTERVALS = [500, 1_000, 2_000];

/** One private chat, kept across all its turns and checked again after saving. */
export class SoakSetup {
  draftId = "";
  definition: Definition | undefined;
  private revision = 0n;
  private sent: string[] = [];
  /** Sequences whose turn failed during a recorded outage; they were never resent. */
  readonly outageSequences = new Set<string>();
  /** Called after every finished turn with the next turn index, for checkpoints. */
  onTurn: (next: number) => Promise<void> = async () => {};

  constructor(private readonly page: Page, private readonly client: ApiClient,
    private readonly workspace: string,
    private readonly root: Locator,
    base?: Definition) { this.definition = base; }

  get scope(): string { return `/v1/workspaces/${this.workspace}/drafts/${this.draftId}`; }

  /** Continue an existing chat from its durable state, as a resumed journey does. */
  async hydrate(draftId: string): Promise<void> {
    this.draftId = draftId;
    await this.resync();
  }

  private async resync(): Promise<void> {
    const draft = DraftSchema.parse(await this.client.get<unknown>(this.scope));
    expect(draft.candidate, "a resumed setup has a validated candidate").toBeDefined();
    this.revision = BigInt(draft.candidate!.revision);
    this.definition = draft.candidate!.definition;
    const rows = await draftHistory(this.client, this.scope);
    this.sent = rows.filter(row => row.role === MESSAGE_ROLE.user).map(row => row.text ?? "");
  }

  async converse(count: number, savedRevision: number, name: string, from = 0): Promise<void> {
    let consecutive = 0;
    for (let index = from; index < count; index++) {
      const request: SetupRequest = savedRevision === 1 && index === 0
        ? {text: initialSetupMessage(name), edits: true}
        : setupMessage(index, savedRevision);
      const text = savedRevision === 1 && index === 0 ? request.text : `For fleet ${name}: ${request.text}`;
      // Checkpoint past this turn before sending it: a crash mid-send resumes at the next turn, never a resend.
      await this.onTurn(index + 1);
      const started = Date.now();
      const at = new Date(started).toISOString();
      try {
        await this.send(text, request, savedRevision);
      } catch (error) {
        const failedAt = Date.now();
        const outage = outageWatch.during(started, failedAt);
        // The failure record names everything needed to find the turn again.
        const record = {fleet: name, stage: "setup", setup: savedRevision,
          turn: index + 1, sequence: this.sent.length, input: text, sentAt: at, failedAt: new Date(failedAt).toISOString(),
          workspace: this.workspace, draft: this.draftId || null, url: this.page.url(),
          requestId: Reflect.get(Object(error), "request_id") ?? null, error: String(error),
          classification: classify(outage),
          outage: outage ? {kind: outage.kind, failedProbes: outage.failedProbes.length} : null};
        console.error(JSON.stringify({soakFailure: record}));
        consecutive += 1;
        if (!this.draftId || consecutive >= MAX_CONSECUTIVE_FAILURES) throw error;
        // A failed turn stays failed and is never resent; the next turn continues the same chat.
        Object.assign(record, {pending: pendingRequests(this.page), screenshot: await captureFailure(this.page, `${name}-setup${savedRevision}-turn${index + 1}`)});
        outageLedger.push(record);
        if (outage) await outageWatch.waitHealthy();
        // A first chat lives at New fleet without its draft in the URL; a reload would open a blank chat.
        await this.page.goto(draftHref(this.workspace, this.draftId));
        await this.resync();
        // Mark the turn only if the server stored it; a lost send leaves no row to excuse.
        const stored = this.sent.at(-1) === text;
        if (stored) this.outageSequences.add(String(this.sent.length));
        const reply = stored ? (await draftHistory(this.client, this.scope)).find(row =>
          row.role === MESSAGE_ROLE.assistant && row.sequence === String(this.sent.length)) : undefined;
        Object.assign(record, {stored, replyState: reply?.state ?? null, replyFailure: reply?.failure ?? null});
        console.error(JSON.stringify({soakFailureSettled: record}));
        await this.onTurn(index + 1);
        continue;
      }
      consecutive = 0;
      console.log(JSON.stringify({fleet: name, setup: savedRevision, turn: index + 1, status: "PASS",
        ...(request.check ? {unicode: request.check.kind} : {})}));
      await this.onTurn(index + 1);
    }
    await this.assertHistory();
  }

  private async send(text: string, request: SetupRequest, savedRevision: number): Promise<void> {
    const created = this.draftId ? null : this.page.waitForResponse(response =>
      response.request().method() === "POST" && new URL(response.url()).pathname.endsWith(`/workspaces/${this.workspace}/drafts`));
    await this.root.getByLabel(CHAT_COPY.composer).getByRole("textbox").fill(text);
    await this.root.getByRole("button", {name: CHAT_COPY.send, exact: true}).click();
    if (created) {
      const response = await created;
      expect(response.status(), "private setup created").toBe(201);
      this.draftId = DraftSchema.shape.id.parse((await response.headerValue("location"))?.split("/").at(-1));
      console.log(JSON.stringify({soakDraft: {workspace: this.workspace, draft: this.draftId}}));
      // The new route can replace the response before Chromium exposes its body.
      // Native creation starts blank drafts at zero and fleet copies at one.
      this.revision = savedRevision === 1 ? 0n : 1n;
    }
    this.sent.push(text);
    const sequence = String(this.sent.length);
    await expect.poll(async () => {
      const rows = await draftHistory(this.client, this.scope);
      return rows.find(row => row.role === MESSAGE_ROLE.assistant && row.sequence === sequence)?.state ?? "absent";
    }, {timeout: SOAK.turnTimeoutMs, intervals: POLL_INTERVALS}).toMatch(TERMINAL);
    const rows = await draftHistory(this.client, this.scope);
    const reply = rows.find(row => row.role === MESSAGE_ROLE.assistant && row.sequence === sequence);
    expect(reply?.failure, `setup turn ${sequence} failure`).toBeUndefined();
    expect(reply?.state).toBe(MESSAGE_STATE.completed);
    expect(reply?.text?.trim().length).toBeGreaterThan(0);
    await this.assertCandidate(request.edits, request.check);
    await expect(this.root.getByRole("button", {name: CHAT_COPY.send, exact: true})).toBeVisible();
    // Compare visible text with rendered Markdown, while history keeps exact source.
    const visible = await visibleReply(this.page, reply!.text!);
    const row = this.root.locator(`[data-message-id="${sequence}:${MESSAGE_ROLE.assistant}"] [data-role="assistant"]`);
    await expect(row).toBeVisible();
    await expandIfFolded(this.root.locator(`[data-message-id="${sequence}:${MESSAGE_ROLE.assistant}"]`));
    await expect(row).toContainText(visible);
  }

  private async assertCandidate(edits: boolean, check?: InstructionCheck): Promise<void> {
    const before = this.definition?.instructions_md;
    const draft = DraftSchema.parse(await this.client.get<unknown>(this.scope));
    expect(draft.candidate, "setup has a validated candidate").toBeDefined();
    const current = BigInt(draft.candidate!.revision);
    if (edits) expect(current, "requested change advanced the candidate").toBeGreaterThan(this.revision);
    else expect(current, "an explanation preserved the candidate").toBe(this.revision);
    this.revision = current;
    this.definition = draft.candidate!.definition;
    if (check) expectInstructions(before, this.definition.instructions_md, check);
  }

  async assertHistory(): Promise<void> {
    const rows = await draftHistory(this.client, this.scope);
    const users = rows.filter(row => row.role === MESSAGE_ROLE.user);
    const replies = rows.filter(row => row.role === MESSAGE_ROLE.assistant);
    expect(users.map(row => row.text)).toEqual(this.sent);
    expect(replies).toHaveLength(this.sent.length);
    expect(new Set(replies.map(row => row.sequence)).size).toBe(this.sent.length);
    for (const reply of replies) {
      // An outage turn is reported from the ledger; its reply is not required to complete.
      if (this.outageSequences.has(reply.sequence)) continue;
      expect(reply.state).toBe(MESSAGE_STATE.completed);
      expect(reply.failure).toBeUndefined();
      expect(reply.text?.trim().length).toBeGreaterThan(0);
    }
  }
}

/** A Unicode edit keeps every other instruction byte and stores its own text exactly. */
export function expectInstructions(before: string | undefined, after: string, check: InstructionCheck): void {
  expect(before, "the Unicode edit has a base candidate").toBeDefined();
  expect(after, "no replacement characters were stored").not.toContain("\uFFFD");
  if (check.kind === INSTRUCTION_CHECK.append) {
    expect(after.slice(0, before!.length), "every earlier instruction byte is kept").toBe(before);
    expect(after.slice(before!.length), "the Unicode text is appended exactly").toContain(check.text);
    return;
  }
  expect(before!.split(check.from).length - 1, "the replaced Unicode text occurs once").toBe(1);
  expect(after, "only the named Unicode text changed").toBe(before!.replace(check.from, () => check.to));
}
