import { useId } from "react";
import { Link } from "react-router-dom";
import { Button, Card, Label, Nav, Textarea } from "@agentsfleet/design-system";
import { PUBLIC_ACCESS_URL } from "../config";
import { EXAMPLE_ANCHOR, JOBS_ANCHOR, JOB_ID, REQUEST_MAX_LENGTH, type ExampleJob } from "../lib/example-jobs";
import { HERO_HEADLINE } from "../lib/marketing-copy";
import { JobSuggestions } from "./JobSuggestions";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";

type JobComposerProps = {
  request: string;
  onRequest: (request: string) => void;
  job: ExampleJob;
  onJob: (job: ExampleJob) => void;
};

export function JobComposer({ request, onRequest, job, onJob }: JobComposerProps) {
  const id = useId();
  const exampleLabel = job.id === JOB_ID.tests ? "Explore a test regression example" : job.id === JOB_ID.agent ? "Explore a recurring work example" : "Explore an example run";
  return (
    <div id={JOBS_ANCHOR} className="entrance-composer-wrap">
      <Card asChild className="job-composer ph-no-capture">
        <div>
          <Label htmlFor={id} className="sr-only">{HERO_HEADLINE}</Label>
          <Textarea id={id} value={request} onChange={(event) => onRequest(event.target.value)} rows={2}
            placeholder="What needs doing? An incident, a failing test, a recurring job…"
            maxLength={REQUEST_MAX_LENGTH} aria-describedby={`${id}-privacy`} className="job-request ph-no-capture" />
          <div className="composer-actions">
            <span>No tools connected</span>
            <Button asChild size="icon" className="composer-start">
              <a href={PUBLIC_ACCESS_URL} aria-label="Join waitlist" target="_blank" rel="noopener noreferrer"
                onClick={() => trackSignupStarted({ source: "composer_create_fleet", surface: "hero", mode: "humans" })}><span aria-hidden="true">↑</span></a>
            </Button>
          </div>
        </div>
      </Card>
      <JobSuggestions onJob={onJob} />
      {request.trim() && <Nav aria-label="Conversation context" className="entrance-links">
        <Button asChild variant="ghost" size="sm">
          <Link to={`/#${EXAMPLE_ANCHOR}`} onClick={() => trackNavigationClicked({ source: "composer_example", surface: "hero", target: job.id })}>{exampleLabel}</Link>
        </Button>
      </Nav>}
      <p id={`${id}-privacy`} className="signup-continuity">Keep requests general. The waitlist opens in a new tab; your text stays on this page.</p>
    </div>
  );
}
