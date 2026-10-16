import { useCallback, useEffect, useRef, useState } from "react";
import { BrandMark, Button } from "@agentsfleet/design-system";
import type { ExampleJob } from "../lib/example-jobs";
import type { StoryFrame } from "../lib/chat-story";
import { StoryMessage } from "./StoryMessage";
import { useChatScroll } from "./use-chat-scroll";

// The illustration has its own scroll region; it never scrolls the page or edits the composer.
export function IncidentFlow({ job, frame, running, onInspect, onFollowLatest }: { job: ExampleJob; frame: StoryFrame; running: boolean; onInspect: () => void; onFollowLatest: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const evidenceRef = useRef<HTMLElement>(null);
  const citationRef = useRef<HTMLButtonElement | null>(null);
  const previousScroll = useRef<number | null>(null);
  const [adjustScroll, setAdjustScroll] = useState<(inspecting: boolean) => void>(() => () => {});
  const attach = useCallback((element: HTMLDivElement | null) => {
    ref.current = element;
    if (!element) return;
    setAdjustScroll(() => (inspecting: boolean) => {
      if (inspecting && evidenceRef.current) {
        previousScroll.current ??= element.scrollTop;
        element.scrollTop += evidenceRef.current.getBoundingClientRect().top - element.getBoundingClientRect().top;
      } else if (previousScroll.current !== null) {
        element.scrollTop = previousScroll.current;
        previousScroll.current = null;
      }
    });
  }, []);
  const scroll = useChatScroll(ref, frame, running);
  const [selected, setSelected] = useState<number | null>(null);
  const source = selected === null ? undefined : job.evidence[selected];
  useEffect(() => adjustScroll(selected !== null), [selected, adjustScroll]);
  const select = (index: number, citation: HTMLButtonElement) => {
    citationRef.current = citation;
    setSelected(index); onInspect();
  };
  const close = () => { setSelected(null); citationRef.current?.focus({ preventScroll: true }); };
  return (
    <div className="incident-film-wrap"><figure className="incident-figure" data-cycle={frame.cycle}
      onFocusCapture={onInspect}
      aria-label="Illustrated example investigation">
      {/* oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- scrollable log must be keyboard-reachable (WCAG 2.1.1) */}
      <div ref={attach} role="log" tabIndex={0} className="incident-film" data-playing={running} onScroll={scroll.onScroll} aria-label="Example conversation" aria-live="off">
      <div className="fleet-conversation">
        <p className="chat-context">Describe → refine → save → work</p>
        {frame.entries.map((entry, index) => <StoryMessage key={index} entry={entry} job={job} onEvidence={select} />)}
        {frame.pending && <div className="chat-working"><BrandMark className="chat-loader" /><span>{frame.pending.activity}…</span></div>}
        {source && <section ref={evidenceRef} className="inline-evidence" aria-label="Sample evidence">
          <div><span>{source.source}</span><Button variant="ghost" onClick={close}>Close evidence</Button></div>
          <pre>{source.excerpt}</pre><p>{source.detail}</p>{source.url && <a href={source.url} target="_blank" rel="noopener noreferrer">View repository source <span aria-hidden="true">↗</span></a>}<p>Run output is illustrative; linked repository code is real.</p>
        </section>}
      </div>
      </div>
    </figure>{running && !scroll.following && <Button size="sm" variant="outline" className="chat-jump-latest" onClick={() => { scroll.jump(); onFollowLatest(); }}>Jump to latest <span aria-hidden="true">↓</span></Button>}</div>
  );
}
