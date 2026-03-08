import { DOCS_QUICKSTART_URL, MAILTO_TEAM_PILOT } from "../config";

export default function CTABlock() {
  return (
    <div className="cta-block">
      <h2>Queue work. Review PRs. Sleep.</h2>
      <p>Start with Hobby, then move to Team when you need custom profiles, deeper observability, and tighter execution controls.</p>
      <div className="cta-row">
        <a className="cta" href={DOCS_QUICKSTART_URL}>
          Start free
        </a>
        <a className="cta ghost" href={MAILTO_TEAM_PILOT}>
          Book team pilot
        </a>
      </div>
    </div>
  );
}
