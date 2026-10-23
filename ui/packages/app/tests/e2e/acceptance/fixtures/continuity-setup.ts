import { expect, type Locator, type Page } from "@playwright/test";
import { DraftSchema, MESSAGE_ROLE, MESSAGE_STATE, type Definition } from "@/lib/api/draft-types";
import { type ApiClient } from "./api-client";
import { draftHistory } from "./continuity-history";
import { CONTINUITY, initialSetupMessage, setupMessage } from "./continuity-scenario";
import { CHAT_COPY } from "./private-chat";
import { visibleReply } from "./continuity-visible";

const TERMINAL = /^(completed|failed|timed_out|cancelled)$/;
const POLL_INTERVALS = [500, 1_000, 2_000];

/** One private chat, kept across all its turns and checked again after saving. */
export class ContinuitySetup {
  draftId = "";
  definition: Definition | undefined;
  private revision = 0n;
  private readonly sent: string[] = [];

  constructor(private readonly page: Page, private readonly client: ApiClient,
    private readonly workspace: string,
    private readonly root: Locator) {}

  get scope(): string { return `/v1/workspaces/${this.workspace}/drafts/${this.draftId}`; }

  async converse(count: number, savedRevision: number, name: string): Promise<void> {
    for (let index = 0; index < count; index++) {
      const request = savedRevision === 1 && index === 0
        ? {text: initialSetupMessage(name), edits: true}
        : setupMessage(index, savedRevision);
      const text = savedRevision === 1 && index === 0 ? request.text : `For fleet ${name}: ${request.text}`;
      await this.send(text, request.edits, savedRevision);
      console.log(JSON.stringify({fleet: name, setup: savedRevision, turn: index + 1, status: "PASS"}));
    }
    await this.assertHistory();
  }

  private async send(text: string, edits: boolean, savedRevision: number): Promise<void> {
    const created = this.draftId ? null : this.page.waitForResponse(response =>
      response.request().method() === "POST" && new URL(response.url()).pathname.endsWith(`/workspaces/${this.workspace}/drafts`));
    await this.root.getByRole("textbox", {name: CHAT_COPY.composerInput}).fill(text);
    await this.root.getByRole("button", {name: CHAT_COPY.send, exact: true}).click();
    if (created) {
      const response = await created;
      expect(response.status(), "private setup created").toBe(201);
      this.draftId = DraftSchema.shape.id.parse((await response.headerValue("location"))?.split("/").at(-1));
      // The new route can replace the response before Chromium exposes its body.
      // Native creation starts blank drafts at zero and fleet copies at one.
      this.revision = savedRevision === 1 ? 0n : 1n;
    }
    this.sent.push(text);
    const sequence = String(this.sent.length);
    await expect.poll(async () => {
      const rows = await draftHistory(this.client, this.scope);
      return rows.find(row => row.role === MESSAGE_ROLE.assistant && row.sequence === sequence)?.state ?? "absent";
    }, {timeout: CONTINUITY.turnTimeoutMs, intervals: POLL_INTERVALS}).toMatch(TERMINAL);
    const rows = await draftHistory(this.client, this.scope);
    const reply = rows.find(row => row.role === MESSAGE_ROLE.assistant && row.sequence === sequence);
    expect(reply?.failure, `setup turn ${sequence} failure`).toBeUndefined();
    expect(reply?.state).toBe(MESSAGE_STATE.completed);
    expect(reply?.text?.trim().length).toBeGreaterThan(0);
    await this.assertCandidate(edits);
    await expect(this.root.getByRole("button", {name: CHAT_COPY.send, exact: true})).toBeVisible();
    // Compare visible text with rendered Markdown, while history keeps exact source.
    const visible = await visibleReply(this.page, reply!.text!);
    const row = this.root.locator(`[data-message-id="${sequence}:${MESSAGE_ROLE.assistant}"] [data-role="assistant"]`);
    await expect(row).toBeVisible();
    await expect(row).toContainText(visible);
  }

  private async assertCandidate(edits: boolean): Promise<void> {
    const draft = DraftSchema.parse(await this.client.get<unknown>(this.scope));
    expect(draft.candidate, "setup has a validated candidate").toBeDefined();
    const current = BigInt(draft.candidate!.revision);
    if (edits) expect(current, "requested change advanced the candidate").toBeGreaterThan(this.revision);
    else expect(current, "an explanation preserved the candidate").toBe(this.revision);
    this.revision = current;
    this.definition = draft.candidate!.definition;
  }

  async assertHistory(): Promise<void> {
    const rows = await draftHistory(this.client, this.scope);
    const users = rows.filter(row => row.role === MESSAGE_ROLE.user);
    const replies = rows.filter(row => row.role === MESSAGE_ROLE.assistant);
    expect(users.map(row => row.text)).toEqual(this.sent);
    expect(replies).toHaveLength(this.sent.length);
    expect(new Set(replies.map(row => row.sequence)).size).toBe(this.sent.length);
    for (const reply of replies) {
      expect(reply.state).toBe(MESSAGE_STATE.completed);
      expect(reply.failure).toBeUndefined();
      expect(reply.text?.trim().length).toBeGreaterThan(0);
    }
  }
}
