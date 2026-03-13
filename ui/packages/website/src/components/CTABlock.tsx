import { DOCS_QUICKSTART_URL, MAILTO_TEAM_PILOT } from "../config";
import { trackSignupCompleted, trackTeamPilotBookingStarted } from "../analytics/posthog";

export default function CTABlock() {
  return (
    <div className="cta-block">
      <h2>Queue work. Review PRs. Sleep.</h2>
      <p>Start with Hobby, then move to Team when you need custom profiles, deeper observability, and tighter execution controls.</p>
      <div className="cta-row">
        <a
          className="cta"
          href={DOCS_QUICKSTART_URL}
          onClick={() => trackSignupCompleted({ source: "cta_block_start_free", surface: "cta_block", mode: "humans" })}
        >
          Start free
        </a>
        <a
          className="cta ghost"
          href={MAILTO_TEAM_PILOT}
          onClick={() => trackTeamPilotBookingStarted({ source: "cta_block_team_pilot", surface: "cta_block", mode: "humans" })}
        >
          Book team pilot
        </a>
      </div>
    </div>
  );
}
