import { useMemo, useRef, type ReactNode } from "react";
import { Button, Card, DisplayLG, Section } from "@agentsfleet/design-system";
import type { ExampleJob } from "../lib/example-jobs";
import { EXAMPLE_ANCHOR, EXAMPLE_REPOSITORY_URL } from "../lib/example-jobs";
import { PUBLIC_ACCESS_URL } from "../config";
import { createChatStory, storyFrame } from "../lib/chat-story";
import { useSceneMotion } from "./use-scene-motion";
import { IncidentFlow } from "./IncidentFlow";
import {
  trackInteractionClicked,
  trackNavigationClicked,
  trackSignupStarted,
} from "../analytics/posthog";

export default function HowItWorks({
  job,
  navigation,
}: {
  job: ExampleJob;
  navigation?: ReactNode;
}) {
  const ref = useRef<HTMLElement>(null);
  const control = useRef<HTMLButtonElement>(null);
  const motion = useSceneMotion(ref);
  const story = useMemo(() => createChatStory(job), [job]);
  const frame = storyFrame(story, motion.elapsed, motion.complete);
  return (
    <Section asChild>
      <section
        ref={ref}
        id={EXAMPLE_ANCHOR}
        className="mb-xl"
        data-testid="how-it-works"
        data-running={motion.running}
      >
        <Card asChild className="overflow-hidden p-0">
          <div>
            <div
              data-testid="example-heading"
              className="flex flex-wrap items-center gap-md border-b border-border px-xl py-md"
            >
              {navigation}
              <DisplayLG className="order-last basis-full text-body font-semibold md:order-none md:basis-auto md:flex-1">
                {job.title}
              </DisplayLG>
              <span className="ml-auto text-label text-text-muted">
                Example run
              </span>
            </div>
            <IncidentFlow
              key={motion.resumeGeneration}
              job={job}
              frame={frame}
              running={motion.running}
              onInspect={motion.pause}
              onFollowLatest={() =>
                control.current?.focus({ preventScroll: true })
              }
            />
            <div className="flex flex-wrap items-center justify-between gap-md border-t border-border p-md sm:px-xl">
              <span className="text-label text-text-muted">
                {frame.finished
                  ? "Report ready · repository unchanged"
                  : "Example conversation"}
              </span>
              <div className="flex w-full flex-wrap gap-sm sm:w-auto">
                {!motion.staticMode && (
                  <Button
                    ref={control}
                    variant="ghost"
                    size="sm"
                    className="min-h-11"
                    onClick={motion.toggle}
                  >
                    {motion.playing ? "Pause" : "Play"}
                  </Button>
                )}
                {!motion.complete && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="min-h-11"
                    onClick={() => {
                      motion.readAll();
                      control.current?.focus({ preventScroll: true });
                      trackInteractionClicked({
                        source: "example_read_full",
                        surface: "example",
                        target: job.id,
                      });
                    }}
                  >
                    Read full conversation
                  </Button>
                )}
                <Button asChild size="sm" className="min-h-11">
                  <a
                    href={PUBLIC_ACCESS_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={() =>
                      trackSignupStarted({
                        source: "example_create_fleet",
                        surface: "example",
                        mode: "humans",
                      })
                    }
                  >
                    Join waitlist <span aria-hidden="true">↗</span>
                  </a>
                </Button>
              </div>
            </div>
          </div>
        </Card>
        <p className="mt-md text-label text-text-muted">
          Example conversation with{" "}
          <a
            href={EXAMPLE_REPOSITORY_URL}
            className="text-pulse underline underline-offset-4"
            target="_blank"
            rel="noopener noreferrer"
            onClick={() =>
              trackNavigationClicked({
                source: "example_repository",
                surface: "example",
                target: "github",
              })
            }
          >
            agentsfleet/linkwarden ↗
          </a>{" "}
          · sample run.
        </p>
      </section>
    </Section>
  );
}
