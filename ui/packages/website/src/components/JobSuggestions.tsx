import type { ExampleJob } from "../lib/example-jobs";
import { Button } from "@agentsfleet/design-system";
import { ClockIcon } from "@phosphor-icons/react/dist/ssr/Clock";
import { PulseIcon } from "@phosphor-icons/react/dist/ssr/Pulse";
import { TestTubeIcon } from "@phosphor-icons/react/dist/ssr/TestTube";
import { Link } from "react-router-dom";
import { trackInteractionClicked } from "../analytics/posthog";
import { EXAMPLE_ANCHOR, EXAMPLE_JOBS, JOB_ID } from "../lib/example-jobs";

const SUGGESTION_ICONS = {
  [JOB_ID.incident]: PulseIcon,
  [JOB_ID.tests]: TestTubeIcon,
  [JOB_ID.agent]: ClockIcon,
} as const;

export function JobSuggestions({ onJob }: { onJob: (job: ExampleJob) => void }) {
  return (
    <div className="mt-lg flex flex-wrap gap-md" aria-label="Ideas for your fleet">
      {EXAMPLE_JOBS.map((job) => {
        const Icon = SUGGESTION_ICONS[job.id];
        return (
          <Button key={job.id} asChild wrap variant="outline" className="min-h-11 rounded-full">
            <Link
              to={`#${EXAMPLE_ANCHOR}`}
              aria-label={`${job.label} · Explore example run`}
              onClick={() => {
                onJob(job);
                trackInteractionClicked({ source: `hero_suggestion_${job.id}`, surface: "hero", target: job.id });
              }}
            >
              <Icon aria-hidden="true" />
              {job.label}
            </Link>
          </Button>
        );
      })}
    </div>
  );
}
