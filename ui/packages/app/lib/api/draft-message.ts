import type { Source } from "./draft-types";
import { AcceptedSchema, CreatedMessageSchema } from "./draft-types";
import { REQUEST_METHOD, setupRequest } from "./draft-request";
import { saveRequested } from "./draft-chat-save";

type Input = {id?: string; source: Source; model: string; message: string};

export async function admitDraftMessage(base: string, input: Input, operation: string, signal: AbortSignal) {
  const body = {message: input.message, operation_id: operation, save: saveRequested(input.message)};
  if (input.id) {
    await setupRequest({path: `${base}/drafts/${input.id}/messages`, method: REQUEST_METHOD.post, body}, AcceptedSchema, signal);
    return undefined;
  }
  return setupRequest({path: `${base}/drafts`, method: REQUEST_METHOD.post,
    body: {...body, source: input.source, authoring_model_ref: input.model}}, CreatedMessageSchema, signal);
}
