import { useCallback, useEffect, useRef, useState } from "react";
import { SignalSpinner, Button } from "@agentsfleet/design-system";
import type { ExampleJob } from "../lib/example-jobs";
import type { StoryFrame } from "../lib/chat-story";
import { StoryMessage } from "./StoryMessage";
import { useChatScroll } from "./use-chat-scroll";

// The illustration has its own scroll region; it never scrolls the page or edits the composer.
export function IncidentFlow({
  job,
  frame,
  running,
  onInspect,
  onFollowLatest,
}: {
  job: ExampleJob;
  frame: StoryFrame;
  running: boolean;
  onInspect: () => void;
  onFollowLatest: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const evidenceRef = useRef<HTMLElement>(null);
  const citationRef = useRef<HTMLButtonElement | null>(null);
  const previousScroll = useRef<number | null>(null);
  const [adjustScroll, setAdjustScroll] = useState<
    (inspecting: boolean) => void
  >(() => () => {});
  const attach = useCallback((element: HTMLDivElement | null) => {
    ref.current = element;
    if (!element) return;
    setAdjustScroll(() => (inspecting: boolean) => {
      if (inspecting && evidenceRef.current) {
        previousScroll.current ??= element.scrollTop;
        element.scrollTop +=
          evidenceRef.current.getBoundingClientRect().top -
          element.getBoundingClientRect().top;
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
    setSelected(index);
    onInspect();
  };
  const close = () => {
    setSelected(null);
    citationRef.current?.focus({ preventScroll: true });
  };
  return (
    <div className="relative">
      <figure
        data-cycle={frame.cycle}
        onFocusCapture={onInspect}
        aria-label="Illustrated example investigation"
      >
        {/* oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- scrollable log must be keyboard-reachable (WCAG 2.1.1) */}
        <div ref={attach} role="log" tabIndex={0}
          className="group/chat h-80 overflow-y-auto overscroll-contain bg-background p-lg [scrollbar-width:thin] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring md:h-[clamp(17.5rem,46vh,27.5rem)] md:p-xl"
          data-playing={running}
          onScroll={scroll.onScroll}
          aria-label="Example conversation"
          aria-live="off"
        >
          <div className="mx-auto max-w-chat">
            <p className="mb-xl font-mono text-xs text-text-muted">
              Describe → refine → save → work
            </p>
            {frame.entries.map((entry, index) => (
              <StoryMessage
                key={index}
                entry={entry}
                job={job}
                onEvidence={select}
              />
            ))}
            {frame.pending && (
              <div className="flex min-h-11 items-center gap-md text-label text-text-muted">
                <SignalSpinner className="size-6 text-pulse group-data-[playing=true]/chat:motion-safe:animate-spin" />
                <span>{frame.pending.activity}…</span>
              </div>
            )}
            {source && (
              <section
                ref={evidenceRef}
                className="grid gap-lg rounded-md border border-border-strong bg-card p-lg text-label text-text-muted"
                aria-label="Sample evidence"
              >
                <div className="flex items-center justify-between gap-md">
                  <span>{source.source}</span>
                  <Button variant="ghost" className="min-h-11" onClick={close}>
                    Close evidence
                  </Button>
                </div>
                <pre className="font-mono text-xs whitespace-pre-wrap">
                  {source.excerpt}
                </pre>
                <p>{source.detail}</p>
                {source.url && (
                  <a
                    className="text-pulse underline underline-offset-4"
                    href={source.url}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    View repository source <span aria-hidden="true">↗</span>
                  </a>
                )}
                <p>
                  Run output is illustrative; linked repository code is real.
                </p>
              </section>
            )}
          </div>
        </div>
      </figure>
      {running && !scroll.following && (
        <Button
          size="sm"
          variant="outline"
          className="absolute bottom-md left-1/2 min-h-11 -translate-x-1/2 bg-card"
          onClick={() => {
            scroll.jump();
            onFollowLatest();
          }}
        >
          Jump to latest <span aria-hidden="true">↓</span>
        </Button>
      )}
    </div>
  );
}
