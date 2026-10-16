import { useMemo, useRef, type ReactNode } from "react";
import { Button, Card, DisplayLG, Section } from "@agentsfleet/design-system";
import type { ExampleJob } from "../lib/example-jobs";
import { EXAMPLE_ANCHOR, EXAMPLE_REPOSITORY_URL } from "../lib/example-jobs";
import { APP_SIGN_UP_URL } from "../config";
import { createChatStory, storyFrame } from "../lib/chat-story";
import { useSceneMotion } from "./use-scene-motion";
import { IncidentFlow } from "./IncidentFlow";
import { trackInteractionClicked, trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";

export default function HowItWorks({ job, navigation }: { job: ExampleJob; navigation?: ReactNode }) {
  const ref = useRef<HTMLElement>(null);
  const control = useRef<HTMLButtonElement>(null);
  const motion = useSceneMotion(ref);
  const story = useMemo(() => createChatStory(job), [job]);
  const frame = storyFrame(story, motion.elapsed, motion.complete);
  return (
    <Section asChild>
      <section ref={ref} id={EXAMPLE_ANCHOR} className="workbench-section" data-testid="how-it-works" data-running={motion.running}>
        <Card asChild className="workbench p-0"><div>
          <div className="workbench-bar">{navigation}<DisplayLG>{job.title}</DisplayLG><span className="example-badge">Example run</span></div>
          <IncidentFlow key={motion.resumeGeneration} job={job} frame={frame} running={motion.running} onInspect={motion.pause} onFollowLatest={() => control.current?.focus({ preventScroll: true })} />
          <div className="scene-controls">
            <span>{frame.finished ? "Report ready · repository unchanged" : "One conversation, from idea to work"}</span>
            <div>
              {!motion.staticMode && <Button ref={control} variant="ghost" size="sm" onClick={motion.toggle}>{motion.playing ? "Pause" : "Play"}</Button>}
              {!motion.complete && <Button variant="ghost" size="sm" onClick={() => { motion.readAll(); control.current?.focus({ preventScroll: true }); trackInteractionClicked({ source: "example_read_full", surface: "example", target: job.id }); }}>Read full conversation</Button>}
              <Button asChild size="sm"><a href={APP_SIGN_UP_URL} target="_blank" rel="noopener noreferrer"
                onClick={() => trackSignupStarted({ source: "example_create_fleet", surface: "example", mode: "humans" })}>Create fleet <span aria-hidden="true">↗</span></a></Button>
            </div>
          </div>
        </div></Card>
        <p className="workbench-footnote">Example conversation with <a href={EXAMPLE_REPOSITORY_URL} target="_blank" rel="noopener noreferrer"
          onClick={() => trackNavigationClicked({ source: "example_repository", surface: "example", target: "github" })}>agentsfleet/linkwarden ↗</a> · sample run, no changes applied.</p>
      </section>
    </Section>
  );
}
