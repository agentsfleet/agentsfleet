import { Link } from "react-router-dom";
import { Button, DisplayXL, cn } from "@agentsfleet/design-system";

import { SECTION_SPACING, SITE_CONTAINER } from "../components/site-classes";

export default function NotFound() {
  return (
    <section className={SECTION_SPACING}>
      <div className={cn(SITE_CONTAINER, "grid justify-items-start gap-xl")}>
        <p className="font-mono text-label text-text-muted">404</p>
        <DisplayXL className="text-fluid-display-lg max-w-narrow font-bold">
          This page isn’t here.
        </DisplayXL>
        <p className="max-w-measure text-body-lg text-text-muted">
          The link may have changed. Head home or browse agent resources.
        </p>
        <div className="flex flex-wrap gap-md">
          <Button asChild className="min-h-11">
            <Link reloadDocument to="/">
              Back to home
            </Link>
          </Button>
          <Button asChild variant="outline" className="min-h-11">
            <Link to="/agents">Explore agents</Link>
          </Button>
        </div>
      </div>
    </section>
  );
}
