import { DisplayLG, Section } from "@agentsfleet/design-system";
import { PUBLIC_ACCESS_URL, DOCS_URL, GITHUB_URL, SELF_HOSTED_URL } from "../config";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";

const SOURCE_SURFACE = "source_strip";
const EXTERNAL_TARGET = "_blank";
const EXTERNAL_REL = "noopener noreferrer";

export function AudienceSection() {
  return (
    <Section asChild><section className="continuity-section" aria-label="Recurring work and controls" data-testid="audience-section">
      <div className="wrap continuity-layout">
        <div><DisplayLG>A job to come back to.<br />Context to build on.</DisplayLG><p className="continuity-lede">Configure a supported schedule, repository event, or Slack mention. Each trigger starts a new run with the job’s defined tools and access.</p><a href={DOCS_URL} className="text-pulse"
          onClick={() => trackNavigationClicked({ source: "continuity_docs", surface: "continuity", target: "docs" })}>Read the product docs ↗</a></div>
        <div className="continuity-details">
          <div><span>01</span><div><h3>Keep the useful context.</h3><p>Workflows with memory tools can save findings for later runs. The Slack assistant uses its channel’s context. This is saved information, not a promise of automatic learning.</p></div></div>
          <div><span>02</span><div><h3>Separate reads from writes.</h3><p>Reading logs, posting a Slack report, and changing code require different access. Choose resources and permissions for the job you enable.</p></div></div>
          <div><span>03</span><div><h3>Inspect the work and the cost.</h3><p>Review run history and configure fleet budgets. An active run can cross a limit before its next budget check.</p></div></div>
        </div>
      </div>
      <div className="wrap source-strip"><span>Built in the open. Inspect the runtime.</span><a href={GITHUB_URL} target={EXTERNAL_TARGET} rel={EXTERNAL_REL}
        onClick={() => trackNavigationClicked({ source: "source_github", surface: SOURCE_SURFACE, target: "github" })}>GitHub ↗</a><span><a href={PUBLIC_ACCESS_URL} target={EXTERNAL_TARGET} rel={EXTERNAL_REL}
        onClick={() => trackSignupStarted({ source: "source_early_access", surface: SOURCE_SURFACE, mode: "humans" })}>Hosted waitlist ↗</a> · <a href={SELF_HOSTED_URL} target={EXTERNAL_TARGET} rel={EXTERNAL_REL}
        onClick={() => trackNavigationClicked({ source: "source_self_hosted", surface: SOURCE_SURFACE, target: "self_hosted" })}>Self hosting in your infrastructure ↗</a></span></div>
    </section></Section>
  );
}
