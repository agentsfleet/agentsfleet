import { useId } from "react";
import { Link } from "react-router-dom";
import { Button, Card, Label, Nav, Textarea } from "@agentsfleet/design-system";
import { PUBLIC_ACCESS_URL } from "../config";
import {
  EXAMPLE_ANCHOR,
  JOBS_ANCHOR,
  JOB_ID,
  REQUEST_MAX_LENGTH,
  type ExampleJob,
} from "../lib/example-jobs";
import { JobSuggestions } from "./JobSuggestions";
import {
  trackNavigationClicked,
  trackSignupStarted,
} from "../analytics/posthog";

type JobComposerProps = {
  request: string;
  onRequest: (request: string) => void;
  job: ExampleJob;
  onJob: (job: ExampleJob) => void;
};

export function JobComposer({
  request,
  onRequest,
  job,
  onJob,
}: JobComposerProps) {
  const id = useId();
  const exampleLabel =
    job.id === JOB_ID.tests
      ? "Explore a test regression example"
      : job.id === JOB_ID.agent
        ? "Explore a recurring work example"
        : "Explore an example run";
  return (
    <div id={JOBS_ANCHOR}>
      <Card asChild className="rounded-lg p-lg ph-no-capture sm:p-xl">
        <div>
          <Label htmlFor={id} className="sr-only">
            What should your fleet handle?
          </Label>
          <Textarea
            id={id}
            value={request}
            onChange={(event) => onRequest(event.target.value)}
            rows={2}
            placeholder="What would you like your fleet to do?"
            maxLength={REQUEST_MAX_LENGTH}
            aria-describedby={`${id}-privacy`}
            className="min-h-chat-input resize-y border-0 bg-transparent p-md text-composer leading-composer shadow-none ph-no-capture"
          />
          <div className="mt-lg flex flex-wrap items-center justify-between gap-md text-label text-text-subtle">
            <span>Example composer</span>
            <Button asChild wrap variant="outline" className="min-h-11">
              <a
                href={PUBLIC_ACCESS_URL}
                target="_blank"
                rel="noopener noreferrer"
                onClick={() =>
                  trackSignupStarted({
                    source: "composer_create_fleet",
                    surface: "hero",
                    mode: "humans",
                  })
                }
              >
                Join waitlist
              </a>
            </Button>
          </div>
        </div>
      </Card>
      <JobSuggestions onJob={onJob} />
      {request.trim() && (
        <Nav
          aria-label="Conversation context"
          className="mt-sm flex flex-wrap gap-sm"
        >
          <Button asChild variant="ghost" size="sm" className="min-h-11">
            <Link
              to={`#${EXAMPLE_ANCHOR}`}
              onClick={() =>
                trackNavigationClicked({
                  source: "composer_example",
                  surface: "hero",
                  target: job.id,
                })
              }
            >
              {exampleLabel}
            </Link>
          </Button>
        </Nav>
      )}
      <p
        id={`${id}-privacy`}
        className="mx-md mt-sm text-label text-text-subtle"
      >
        Your message stays on this page. Pick an example to see a fleet at work.
      </p>
    </div>
  );
}
