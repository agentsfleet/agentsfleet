import { useId } from "react";
import { Button, DisplayLG, DisplayXL, List, ListItem, Section, cn } from "@agentsfleet/design-system";
import { CliQuickstart } from "../components/CliQuickstart";
import { SECTION_SPACING, SITE_CONTAINER } from "../components/site-classes";
import { DOCS_URL } from "../config";

const jsonLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "agentsfleet",
  applicationCategory: "DeveloperApplication",
  url: "https://www.agentsfleet.net/agents",
  sameAs: ["https://www.agentsfleet.net/openapi.json"],
};

const FLEET_COLLECTION = "/v1/workspaces/{workspace_id}/fleets";
const FLEET_PATH = `${FLEET_COLLECTION}/{fleet_id}`;
const API_STEPS = [
  {
    title: "Create a fleet",
    path: `POST ${FLEET_COLLECTION}`,
    description: "Send the fleet’s first message with an operation ID. The response includes the fleet ID.",
  },
  {
    title: "Send a follow-up",
    path: `POST ${FLEET_PATH}/messages`,
    description: "Send another message to the same fleet.",
  },
  {
    title: "Read replies",
    path: `GET ${FLEET_PATH}/turns/stream`,
    description: "Follow the fleet’s replies as it works.",
  },
] as const;

export default function Fleets() {
  const apiHeading = useId();
  return (
    <div data-testid="fleets-page">
      <Section className={cn(SECTION_SPACING, "block")} aria-label="Start a fleet from your terminal">
        <div className={cn(SITE_CONTAINER, "grid items-start gap-2xl lg:grid-cols-[1fr_1.25fr] lg:gap-5xl")}>
          <div className="grid gap-xl">
            <DisplayXL className="text-fluid-display-lg leading-display-lg font-title text-balance">Start a fleet from your terminal</DisplayXL>
            <p className="max-w-prose text-conversation leading-conversation text-text-muted">
              Sign in with the command-line tool and choose a workspace. Send a first message to create a fleet, then keep talking with <code className="font-mono">fleet steer</code>.
            </p>
            <Button asChild wrap variant="outline" className="min-h-11 w-fit"><a href={DOCS_URL}>Read the docs</a></Button>
          </div>
          <CliQuickstart />
        </div>
      </Section>
      <Section variant="divided" className={cn(SECTION_SPACING, "bg-card")} aria-labelledby={apiHeading}>
        <div className={cn(SITE_CONTAINER, "grid gap-2xl lg:grid-cols-[1fr_1.25fr] lg:gap-5xl")}>
          <div className="grid content-start gap-lg">
            <DisplayLG id={apiHeading} className="font-title">Work with the API</DisplayLG>
            <p className="max-w-prose text-conversation leading-conversation text-text-muted">Use an API key with access to your workspace. The OpenAPI file lists the fields and permissions for each request.</p>
            <Button asChild wrap variant="outline" className="min-h-11 w-fit">
              <a href="/openapi.json" data-testid="fleets-openapi-link">OpenAPI JSON</a>
            </Button>
          </div>
          <List variant="ordered" className="list-none space-y-0 divide-y divide-border border-y border-border pl-0">
            {API_STEPS.map(({ title, path, description }) => (
              <ListItem key={title} className="grid gap-md py-xl">
                <h3 className="text-heading font-title">{title}</h3>
                <code className="font-mono text-mono text-pulse">{path}</code>
                <p className="text-conversation leading-conversation text-text-muted">{description}</p>
              </ListItem>
            ))}
          </List>
        </div>
      </Section>
      <script type="application/ld+json">{JSON.stringify(jsonLd)}</script>
    </div>
  );
}
