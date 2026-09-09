/**
 * A reply, split into what the model worked through and what it actually said.
 *
 * Some models emit their reasoning inline, wrapped in `<think>`. The durable
 * row keeps only the answer — `fleet-stream-row.ts` reads `response_text` — but
 * the live chunk stream is the model's raw output and accumulates verbatim
 * (`fleet-stream-frames.ts`). So the same turn read differently depending on
 * whether you watched it arrive or came back to it: mid-stream the operator saw
 * a paragraph of the model talking to itself, and after a navigation it was
 * gone. Splitting here settles that. Both paths render the same answer, and the
 * reasoning becomes something to open rather than something to read past.
 */

const OPEN = "<think>";
const CLOSE = "</think>";

export type ReplyParts = {
  /** What the model worked through, joined when it thought more than once. */
  reasoning: string;
  /** What it said — the reply proper. */
  answer: string;
  /** A block still open: the model is thinking right now, this frame. */
  thinking: boolean;
};

/**
 * Separate the reasoning blocks from the answer.
 *
 * Tolerant of a half-arrived block by design. A stream is read one chunk at a
 * time, so an opening tag with no closing one yet is the normal state for as
 * long as the model is reasoning — everything after it is reasoning so far, and
 * `thinking` says the block is still open.
 */
export function splitReasoning(reply: string): ReplyParts {
  const first = reply.indexOf(OPEN);
  if (first < 0) return { reasoning: "", answer: reply, thinking: false };
  const reasoning: string[] = [];
  const answer: string[] = [];
  // Scanned by index, never by re-slicing the remainder: `indexOf` takes the
  // position to search from, so every slice taken here is a piece of the
  // output. One pass over the reply, and total allocation is the size of what
  // comes back rather than the size of the reply times the number of blocks.
  let cursor = 0;
  let at = first;
  while (at >= 0) {
    if (at > cursor) answer.push(reply.slice(cursor, at));
    const from = at + OPEN.length;
    const close = reply.indexOf(CLOSE, from);
    if (close < 0) {
      reasoning.push(reply.slice(from));
      return parts(reasoning, answer, true);
    }
    reasoning.push(reply.slice(from, close));
    cursor = close + CLOSE.length;
    at = reply.indexOf(OPEN, cursor);
  }
  answer.push(reply.slice(cursor));
  return parts(reasoning, answer, false);
}

function parts(reasoning: string[], answer: string[], thinking: boolean): ReplyParts {
  return {
    reasoning: reasoning.join("\n\n").trim(),
    answer: answer.join("").trim(),
    thinking,
  };
}
