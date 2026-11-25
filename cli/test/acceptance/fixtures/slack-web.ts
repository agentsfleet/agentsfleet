// Slack's Web API, as the one person the journey posts as: a real mention in
// a real channel, and the thread that answers it.
//
// The token is the caller's and is sent only in the request's own header. No
// failure here carries it: Slack names what went wrong with a short code.

import { Schema } from "effect";

const API = "https://slack.com/api";
const REQUEST_TIMEOUT_MS = 30_000;
const POLL_MS = 3_000;

const RefusedSchema = Schema.Struct({ ok: Schema.Boolean, error: Schema.optionalKey(Schema.String) });
const MessageSchema = Schema.Struct({
  ts: Schema.String,
  text: Schema.optionalKey(Schema.String),
  user: Schema.optionalKey(Schema.String),
});
const PostedSchema = Schema.Struct({ ts: Schema.String, message: MessageSchema });
const RepliesSchema = Schema.Struct({ messages: Schema.Array(MessageSchema) });

export type SlackMessage = typeof MessageSchema.Type;

async function call(method: string, token: string, fields: Readonly<Record<string, string>>): Promise<unknown> {
  const response = await fetch(`${API}/${method}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams(fields),
    redirect: "error",
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (!response.ok) throw new Error(`Slack ${method} answered ${response.status}`);
  const body: unknown = await response.json();
  const answer = Schema.decodeUnknownSync(RefusedSchema)(body);
  if (!answer.ok) throw new Error(`Slack ${method} refused: ${answer.error ?? "no reason given"}`);
  return body;
}

/** Posts `text` in `channel` as the token's person, and answers the message Slack stored. */
export async function postMessage(token: string, channel: string, text: string): Promise<SlackMessage> {
  const posted = Schema.decodeUnknownSync(PostedSchema)(await call("chat.postMessage", token, { channel, text }));
  return { ...posted.message, ts: posted.ts };
}

/** The replies under the message at `ts`, oldest first, without that message. */
export async function threadReplies(token: string, channel: string, ts: string): Promise<ReadonlyArray<SlackMessage>> {
  const thread = Schema.decodeUnknownSync(RepliesSchema)(await call("conversations.replies", token, { channel, ts }));
  return thread.messages.filter((message) => message.ts !== ts);
}

/** The first reply under the message at `ts` that `pick` accepts, once the thread holds one. */
export async function awaitReply(
  token: string, channel: string, ts: string, pick: (reply: SlackMessage) => boolean, timeoutMs: number,
): Promise<SlackMessage> {
  const deadline = Date.now() + timeoutMs;
  let replies: ReadonlyArray<SlackMessage> = [];
  for (;;) {
    replies = await threadReplies(token, channel, ts);
    const reply = replies.find(pick);
    if (reply !== undefined) return reply;
    if (Date.now() >= deadline) break;
    await Bun.sleep(POLL_MS);
  }
  throw new Error(`the Slack thread held no such reply within ${timeoutMs}ms; it holds ${replies.length} replies`);
}
