import { BrandMark, Button } from "@agentsfleet/design-system";
import { Link } from "react-router-dom";
import { EXAMPLE_ANCHOR, EXAMPLE_JOBS, JOB_ID, type ExampleJob, type JobId } from "../lib/example-jobs";
import { trackInteractionClicked } from "../analytics/posthog";

function trackSuggestion(job: ExampleJob) {
  trackInteractionClicked({ source: `hero_suggestion_${job.id}`, surface: "hero", target: job.id });
}

export function JobSuggestions({ onJob }: { onJob: (job: ExampleJob) => void }) {
  return <div className="job-suggestions" aria-label="Ideas for your fleet">
    {EXAMPLE_JOBS.map((job) => <Button key={job.id} asChild variant={job.id === JOB_ID.incident ? "default" : "outline"} size="sm">
      <Link to={`/#${EXAMPLE_ANCHOR}`} onClick={() => { onJob(job); trackSuggestion(job); }}>
        <SuggestionIcon icon={job.id} /><span className="suggestion-copy"><span>{job.label}</span><span>Explore example run →</span></span>
      </Link>
    </Button>)}
  </div>;
}

function SuggestionIcon({ icon }: { icon: JobId }) {
  if (icon === JOB_ID.agent) return <BrandMark className="suggestion-icon" />;
  return <svg className="suggestion-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {icon === JOB_ID.incident ? <><path d="M3 12h4l3-8 4 16 3-8h4" /><circle cx="12" cy="12" r="10" /></> : <><path d="M8 4h8M9 4V2h6v2M6 4H4v18h16V4h-2M8 11l2 2 5-5M8 18h8" /></>}
  </svg>;
}
