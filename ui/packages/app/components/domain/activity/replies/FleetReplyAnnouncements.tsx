import type { ReactNode } from "react";
import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { truncate } from "@/lib/utils";

export const SETTLED_REPLY_STATUS = "settled-reply-status";
// Long enough for a screen reader to queue what the status says; any longer and
// the copy only repeats the reply to someone browsing the page.
export const ANNOUNCEMENT_HOLD_MS = 7_000;
// A settled reply is announced as a summary: the row holds the whole answer,
// and a screen reader reading an essay from a status cannot be skimmed.
export const SPOKEN_REPLY_MAX_CHARS = 200;
// The markdown a summary drops, cheaply: fences, link and image targets, block
// markers, and emphasis marks. An underscore inside a word is kept, so
// `fleet_id` reads as written.
const MARKDOWN_TO_SPOKEN: ReadonlyArray<readonly [RegExp, string]> = [
  [/^[ \t]*(?:```|~~~).*$/gm, ""],
  [/^[ \t]*(?:[-*_][ \t]*){3,}$/gm, ""],
  [/!?\[([^\]]*)\]\([^)]*\)/g, "$1"],
  [/^[ \t]{0,3}(?:#{1,6}[ \t]+|>[ \t]?|[-*+][ \t]+|\d+[.)][ \t]+)/gm, ""],
  [/[*`~]+|(?<!\w)_+|_+(?!\w)/g, ""],
  [/\|/g, " "],
  [/\s+/g, " "],
];

// A reply reports its own settling: only it knows it was seen running. The
// default does nothing, for a reply drawn outside a transcript.
const AnnounceSettled = createContext<(text: string) => void>(() => {});

// Each announcement carries its own number, so the same words twice in a row
// still replace the status's text node and are read again.
type Announcement = { text: string; seq: number };
const SILENT: Announcement = { text: "", seq: 0 };

/**
 * The transcript's one polite status. The log itself stays silent, since a
 * streamed reply rewrites it on every flush; a reply that settles while on
 * screen is read out here once, then cleared. `children` pass through
 * untouched, so an announcement re-renders this status and nothing in the log.
 */
export function SettledReplyStatus({ children }: { children: ReactNode }) {
  const [announcement, setAnnouncement] = useState(SILENT);
  const announce = useCallback((text: string) => setAnnouncement((prev) => ({ text, seq: prev.seq + 1 })), []);
  useEffect(() => {
    if (announcement.text.length === 0) return;
    const cleared = setTimeout(() => setAnnouncement((prev) => ({ text: "", seq: prev.seq })), ANNOUNCEMENT_HOLD_MS);
    return () => clearTimeout(cleared);
  }, [announcement]);
  return (
    <AnnounceSettled value={announce}>
      {children}
      <output className="sr-only" data-testid={SETTLED_REPLY_STATUS}>
        <span key={announcement.seq}>{announcement.text}</span>
      </output>
    </AnnounceSettled>
  );
}

// A reply that mounts settled is history and stays unannounced; one seen open
// is announced when it settles, and never again.
// The summary is made only when it is spoken, never per streamed flush.
export function useSettleAnnouncement(settled: boolean, sender: string, text: string) {
  const announce = useContext(AnnounceSettled);
  const seenOpen = useRef(false);
  useEffect(() => {
    if (!settled) {
      seenOpen.current = true;
      return;
    }
    if (!seenOpen.current) return;
    seenOpen.current = false;
    announce(`${sender}: ${spokenSummary(text)}`);
  }, [settled, sender, text, announce]);
}

/** A reply as a screen reader should hear it: plain words, bounded. */
export function spokenSummary(text: string): string {
  let spoken = text;
  for (const [syntax, replacement] of MARKDOWN_TO_SPOKEN) spoken = spoken.replace(syntax, replacement);
  return truncate(spoken.trim(), SPOKEN_REPLY_MAX_CHARS);
}
