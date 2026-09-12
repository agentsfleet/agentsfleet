import { Link } from "react-router-dom";
import { Button, DisplayXL, Section, SectionLabel } from "@agentsfleet/design-system";

export default function NotFound() {
  return (
    <Section className="site-section">
      <div className="wrap flex flex-col items-start gap-xl">
        <SectionLabel as="p">404 · Page not found</SectionLabel>
        <DisplayXL className="text-fluid-display-lg max-w-narrow">This page isn’t here.</DisplayXL>
        <p className="max-w-measure text-body-lg text-text-muted">The link may have changed. Head home or browse agent resources.</p>
        <div className="flex flex-wrap gap-md">
          <Button asChild><Link to="/">Back to home</Link></Button>
          <Button asChild variant="outline"><Link to="/agents">Explore agents</Link></Button>
        </div>
      </div>
    </Section>
  );
}
