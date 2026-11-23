import { DisplayLG, cn } from "@agentsfleet/design-system";
import {
  PUBLIC_ACCESS_URL,
  DOCS_URL,
  GITHUB_URL,
  SELF_HOSTED_URL,
} from "../config";
import {
  trackNavigationClicked,
  trackSignupStarted,
} from "../analytics/posthog";
import { SECTION_SPACING, SITE_CONTAINER, TEXT_LINK } from "./site-classes";

const benefits = [
  [
    "Keep the useful context.",
    "Workflows with memory tools can save findings for later runs. The Slack assistant uses its channel’s context.",
  ],
  [
    "Separate reads from writes.",
    "Reading logs, posting a Slack report, and changing code require different access. Choose resources and permissions for the job you enable.",
  ],
  [
    "Inspect the work and the cost.",
    "Review run history and configure fleet budgets. An active run can cross a limit before its next budget check.",
  ],
];

const SOURCE_LINK = cn(
  TEXT_LINK,
  "inline-flex min-h-11 items-center text-pulse",
);

export function AudienceSection() {
  return (
    <section
      className={cn(SECTION_SPACING, "border-y border-border bg-card")}
      aria-label="Recurring work and controls"
      data-testid="audience-section"
    >
      <div
        className={cn(SITE_CONTAINER, "grid gap-2xl lg:grid-cols-2 lg:gap-4xl")}
      >
        <div className="grid content-start gap-xl">
          <DisplayLG className="font-bold text-balance">
            A job to come back to.
            <br />
            Context to build on.
          </DisplayLG>
          <p className="max-w-narrow text-body-lg leading-body-lg text-text-muted">
            Configure a supported schedule, repository event, or Slack mention.
            Each trigger starts a new run with the job’s defined tools and
            access.
          </p>
          <a
            href={DOCS_URL}
            className={cn(SOURCE_LINK, "w-fit")}
            onClick={() =>
              trackNavigationClicked({
                source: "continuity_docs",
                surface: "continuity",
                target: "docs",
              })
            }
          >
            Read the product docs ↗
          </a>
        </div>
        <ul className="divide-y divide-border border-y border-border">
          {benefits.map(([title, description]) => (
            <li key={title} className="grid gap-sm py-lg">
              <h3 className="font-bold">{title}</h3>
              <p className="text-text-muted">{description}</p>
            </li>
          ))}
        </ul>
      </div>
      <div
        className={cn(
          SITE_CONTAINER,
          "mt-2xl flex flex-wrap items-center gap-x-2xl text-body-sm",
        )}
      >
        <a
          href={GITHUB_URL}
          target="_blank"
          rel="noopener noreferrer"
          className={SOURCE_LINK}
          onClick={() =>
            trackNavigationClicked({
              source: "source_github",
              surface: "source_strip",
              target: "github",
            })
          }
        >
          GitHub ↗
        </a>
        <a
          href={PUBLIC_ACCESS_URL}
          target="_blank"
          rel="noopener noreferrer"
          className={SOURCE_LINK}
          onClick={() =>
            trackSignupStarted({
              source: "source_early_access",
              surface: "source_strip",
              mode: "humans",
            })
          }
        >
          Join waitlist ↗
        </a>
        <a
          href={SELF_HOSTED_URL}
          target="_blank"
          rel="noopener noreferrer"
          className={SOURCE_LINK}
          onClick={() =>
            trackNavigationClicked({
              source: "source_self_hosted",
              surface: "source_strip",
              target: "self_hosted",
            })
          }
        >
          Self hosting in your infrastructure ↗
        </a>
      </div>
    </section>
  );
}
