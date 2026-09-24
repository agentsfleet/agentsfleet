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
const TOOL_TAGS = [
  { open: "<tool_call>", close: "</tool_call>" },
  { open: "<tool_result>", close: "</tool_result>" },
];

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
  // Remove tool protocol before looking for reasoning delimiters: arguments
  // can themselves contain literal `<think>` or `</think>` text.
  const clean = withoutToolTranscript(reply);
  const first = clean.indexOf(OPEN);
  if (first < 0) return { reasoning: "", answer: clean.trim(), thinking: false };
  const reasoning: string[] = [];
  const answer: string[] = [];
  // Scanned by index, never by re-slicing the remainder: `indexOf` takes the
  // position to search from, so every slice taken here is a piece of the
  // output. One pass over the reply, and total allocation is the size of what
  // comes back rather than the size of the reply times the number of blocks.
  let cursor = 0;
  let at = first;
  while (at >= 0) {
    if (at > cursor) answer.push(clean.slice(cursor, at));
    const from = at + OPEN.length;
    const close = clean.indexOf(CLOSE, from);
    if (close < 0) {
      reasoning.push(clean.slice(from));
      return parts(reasoning, answer, true);
    }
    reasoning.push(clean.slice(from, close));
    cursor = close + CLOSE.length;
    at = clean.indexOf(OPEN, cursor);
  }
  answer.push(clean.slice(cursor));
  return parts(reasoning, answer, false);
}

function parts(reasoning: string[], answer: string[], thinking: boolean): ReplyParts {
  return {
    // Tool payloads already have their own progress rows. Repeating their raw
    // arguments/results in the reasoning disclosure exposes protocol markup
    // (and potentially secret-bearing arguments) in the chat transcript.
    reasoning: reasoning.join("\n\n").replace(/ {2,}/g, " ").trim(),
    answer: answer.join("").trim(),
    thinking,
  };
}

function withoutToolTranscript(text: string): string {
  let visible = "";
  let cursor = 0;
  while (cursor < text.length) {
    let nextAt = -1;
    let nextTag: (typeof TOOL_TAGS)[number] | null = null;
    for (const tag of TOOL_TAGS) {
      const at = text.indexOf(tag.open, cursor);
      if (at >= 0 && (nextAt < 0 || at < nextAt)) {
        nextAt = at;
        nextTag = tag;
      }
    }
    if (!nextTag) {
      visible += text.slice(cursor);
      break;
    }
    visible += text.slice(cursor, nextAt);
    const from = nextAt + nextTag.open.length;
    const end = nextTag.open === "<tool_call>"
      ? closingTagOutsideQuotes(text, from, nextTag.close)
      : closingResultTag(text, from, nextTag.close);
    if (end < from) break;
    cursor = end + nextTag.close.length;
  }
  // A chunk can end halfway through an opening tag. Hold that prefix back so
  // it does not flash in the visible transcript before the next chunk arrives.
  for (const { open } of TOOL_TAGS) {
    for (let length = open.length - 1; length > 0; length -= 1) {
      if (visible.endsWith(open.slice(0, length))) {
        visible = visible.slice(0, -length);
        break;
      }
    }
  }
  return visible;
}

function closingResultTag(text: string, from: number, close: string): number {
  // A result can quote tool-call markup as plain text. Only another result
  // bounds this one; keep the reasoning between two completed results.
  const nextResult = text.indexOf("<tool_result>", from);
  if (nextResult >= 0) {
    const beforeNext = text.lastIndexOf(close, nextResult - 1);
    if (beforeNext >= from) return beforeNext;
  }
  return text.lastIndexOf(close);
}

function closingTagOutsideQuotes(text: string, from: number, close: string): number {
  let quoted = false;
  let escaped = false;
  for (let at = from; at < text.length; at += 1) {
    const char = text[at];
    if (escaped) {
      escaped = false;
    } else if (quoted && char === "\\") {
      escaped = true;
    } else if (char === '"') {
      quoted = !quoted;
    } else if (!quoted && text.startsWith(close, at)) {
      return at;
    }
  }
  return -1;
}
