/**
 * A streaming answer, split into the top-level markdown blocks it has finished
 * and the one still open.
 *
 * A block is finished once two more have started after it. One is not enough:
 * the block right after a list can still turn into the list's next item — a
 * line reading `2` becomes `2.` one chunk later and joins the list above it.
 * Nothing reaches back past a block whose lines have all ended, so the block
 * before that one is final and is parsed once. Only the open tail — the last
 * two blocks, plus whatever the previous flush finished — is parsed again, so
 * a flush costs the size of what is still being written rather than the size
 * of the answer.
 *
 * The boundaries come from the parse the tail's render already runs: its tree
 * is handed back through a remark transformer, and the next flush promotes
 * every block but the last two. The lexer is therefore the one react-markdown
 * renders with (remark with GFM), so a table, a fence with blank lines inside
 * it, or a loose list is one block exactly when the renderer says it is.
 *
 * Two things a whole-document parse resolves across blocks stay unresolved
 * while streaming: a reference link whose definition comes later, and a
 * footnote. A settled reply renders in one parse, so both land with it.
 */

/** A parsed tail, as far as the start offset of each top-level node. */
export type ParsedTail = {
  children: ReadonlyArray<{ position?: { start: { offset?: number } } }>;
};

/** What one flush renders. */
export type StreamingView = {
  /** Finished blocks, in order. Each string keeps its identity once finished. */
  finished: readonly string[];
  /** Everything after the finished blocks, which this flush parses. */
  tail: string;
  /** A remark transformer for the tail's parse: reports where its blocks start. */
  readTail: (tree: ParsedTail) => void;
};

/** Where each finished block of one parsed tail ends — the start of the block after it — as offsets into the text it came from. */
type TailFacts = { text: string; tailStart: number; ends: readonly number[] };

export class StreamingBlocks {
  #finished: readonly string[] = [];
  #finishedSource = "";
  #tailStart = 0;
  #facts: TailFacts | null = null;

  view(text: string): StreamingView {
    // An answer that is no longer an extension of what was finished (the
    // stream was replaced, not appended to) starts over from its first byte.
    if (!text.startsWith(this.#finishedSource)) this.#reset();
    this.#promote(text);
    const tailStart = this.#tailStart;
    return {
      finished: this.#finished,
      tail: text.slice(tailStart),
      readTail: (tree) => {
        this.#facts = readFacts(text, tailStart, tree);
      },
    };
  }

  #reset() {
    this.#finished = [];
    this.#finishedSource = "";
    this.#tailStart = 0;
    this.#facts = null;
  }

  // The previous tail's parse proved every block but its last two finished. They
  // move out of the tail when this text still holds them byte for byte; a
  // parse of text this one does not extend is kept for a later flush.
  #promote(text: string) {
    const facts = this.#facts;
    if (facts === null || facts.tailStart !== this.#tailStart) return;
    const blocks: string[] = [];
    let openStart = this.#tailStart;
    for (const end of facts.ends) {
      blocks.push(facts.text.slice(openStart, end));
      openStart = end;
    }
    const proven = facts.text.slice(this.#tailStart, openStart);
    if (!text.startsWith(proven, this.#tailStart)) return;
    this.#finished = [...this.#finished, ...blocks];
    this.#finishedSource += proven;
    this.#tailStart = openStart;
    this.#facts = null;
  }
}

// Only a tail of three or more blocks finishes anything: block i ends where
// block i + 1 starts, for every block but the last two. A node with no offset
// is a tree no parser here produces; it proves nothing, so nothing moves.
function readFacts(text: string, tailStart: number, tree: ParsedTail): TailFacts | null {
  const ends: number[] = [];
  for (const node of tree.children.slice(1, -1)) {
    const offset = node.position?.start.offset;
    if (offset === undefined) return null;
    ends.push(tailStart + offset);
  }
  return ends.length > 0 ? { text, tailStart, ends } : null;
}
