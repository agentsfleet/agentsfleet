import { useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { Button } from "@agentsfleet/design-system";
import Hero from "../components/Hero";
import HowItWorks from "../components/HowItWorks";
import Pricing from "../components/Pricing";
import { AudienceSection } from "../components/AdoptionSections";
import { DEFAULT_JOB, EXAMPLE_ANCHOR, JOBS_ANCHOR, type ExampleJob } from "../lib/example-jobs";
import { trackNavigationClicked } from "../analytics/posthog";

const EXAMPLE_HASH = `#${EXAMPLE_ANCHOR}`;
const JOBS_HASH = `#${JOBS_ANCHOR}`;

export default function Home() {
  const { hash } = useLocation();
  const [view, setView] = useState({ hash, example: hash === EXAMPLE_HASH });
  const [request, setRequest] = useState("");
  const [job, setJob] = useState<ExampleJob>(DEFAULT_JOB);
  // Anchor navigation inside the page must not dismiss an opened conversation.
  if (hash !== view.hash) {
    const example = hash === EXAMPLE_HASH ? true : hash === JOBS_HASH || hash === "" ? false : view.example;
    setView({ hash, example });
  }
  return (
    <div data-testid="home-page">
      <Hero request={request} onRequest={setRequest} job={job} onJob={setJob}>
        {view.example && <HowItWorks key={job.id} job={job} navigation={<Button asChild variant="ghost" size="sm" className="example-return"><Link to={`/#${JOBS_ANCHOR}`} onClick={() => trackNavigationClicked({ source: "example_back", surface: "example", target: "home" })}><span aria-hidden="true">←</span>Back to home</Link></Button>} />}
      </Hero>
      <AudienceSection />
      <Pricing />
    </div>
  );
}
