import { DisplayXL, Section, SectionLabel } from "@agentsfleet/design-system";
import { trackNavigationClicked } from "../analytics/posthog";
import { SUPPORT_EMAIL } from "../lib/contact";

export default function About() {
  return (
    <Section asChild className="site-section" data-testid="about-page">
      <section aria-label="About agentsfleet">
        <div className="wrap flex flex-col items-start gap-6">
          <SectionLabel className="mb-0">About</SectionLabel>
          <DisplayXL className="text-fluid-display-lg max-w-narrow">More time building. Less time gathering clues.</DisplayXL>
          <div className="max-w-measure flex flex-col gap-5 text-body-lg text-text-muted">
            <p className="m-0">Code reviews and production investigations take attention away from building. agentsfleet brings AI teammates to that recurring work, alongside the tools you already use.</p>
            <p className="m-0">The aim is useful, inspectable work with clear boundaries. You choose access, review the evidence, and decide what ships.</p>
            <p className="m-0">agentsfleet is in early access. We’re looking for solo founders and infrastructure teams to try a real workflow and help us learn what matters.</p>
          </div>
          <div className="max-w-measure border-t border-border pt-6 flex flex-col gap-3">
            <h2 className="m-0 font-sans text-heading font-medium">Tell us what you’re working on.</h2>
            <p className="m-0 text-body text-text-muted">What would you like to delegate, and which tools does it involve? Send a note. No sales form or meeting required.</p>
            <a href={`mailto:${SUPPORT_EMAIL}`} className="inline-flex min-h-11 items-center text-body text-pulse underline underline-offset-4"
              onClick={() => trackNavigationClicked({ source: "about_contact", surface: "about", target: "contact" })}
            >{SUPPORT_EMAIL}</a>
          </div>
        </div>
      </section>
    </Section>
  );
}
