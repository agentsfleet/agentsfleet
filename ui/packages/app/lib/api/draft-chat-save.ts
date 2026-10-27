import type { DraftView } from "./draft-state";
import { DRAFT_STATE, MESSAGE_ROLE, MESSAGE_STATE } from "./draft-types";
import { chatRecovery, isStopped } from "./draft-recovery";
import { SetupUnavailable } from "./draft-request";

// Only standalone commands authorize a write. Questions, quotations and edits stay conversational.
const SAVE_COMMAND = /^(?:please\s+)?(?:go ahead and\s+)?save(?:\s+(?:this|it|the fleet|this fleet|fleet|the setup|this setup|setup|the changes|these changes|changes))?(?:\s+now)?(?:\s+please)?[.!]*$/i;

export class DraftChatSave {
  #operation: string | undefined;
  #committing = false;

  get operation(): string | null { return this.#operation ?? null; }
  request(operation: string, text: string): void {
    this.clear();
    if (SAVE_COMMAND.test(text.trim())) this.#operation = operation;
  }
  clear(): void { this.#operation = undefined; this.#committing = false; }

  observe(view: DraftView, save: () => Promise<void>, fail: (cause: unknown) => void, refresh: () => void): void {
    const operation = this.#operation;
    const draft = view.draft;
    if (!draft || !operation || this.#committing || view.busy || view.pending || view.error || view.inaccessible || view.optimistic) return;
    const user = view.messages.findLast(message => message.role === MESSAGE_ROLE.user);
    if (!user) return;
    if (user.operation_id !== operation) { this.clear(); refresh(); return; }
    const answer = view.messages.find(message => message.role === MESSAGE_ROLE.assistant
      && message.operation_id === operation && message.sequence === user.sequence);
    if (!answer || answer.state === MESSAGE_STATE.queued || answer.state === MESSAGE_STATE.running) return;
    const recovery = chatRecovery(view);
    if (answer.state !== MESSAGE_STATE.completed || isStopped(answer, draft.cancelled_through)
      || recovery.pending || recovery.blocked) { this.clear(); refresh(); return; }
    if (draft.state !== DRAFT_STATE.editing || !draft.review || !draft.candidate) {
      this.clear(); fail(new SetupUnavailable("The model could not prepare a complete setup. Your conversation is retained; keep chatting to finish it.", 409));
      return;
    }
    if (answer.candidate_id !== draft.candidate.id) {
      this.clear(); fail(new SetupUnavailable("The setup changed after the model finalized it. Your draft is retained; try saving again.", 409));
      return;
    }
    // The report pins the finalized candidate; the write rechecks its digest and setup tag.
    // Keep progress visible through the write, and consume once even if later history reads repeat.
    this.#committing = true;
    void save().catch(() => undefined).finally(() => { this.clear(); refresh(); });
  }
}
