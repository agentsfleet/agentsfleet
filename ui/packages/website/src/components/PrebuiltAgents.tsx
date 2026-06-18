import {
  Button,
  Card,
  DisplayLG,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { trackSignupStarted } from "../analytics/posthog";
import { WAITLIST_URL } from "../config";
import {
  AGENT_PILLARS,
  AGENTS_SECTION_HEADING,
  AGENTS_SECTION_LEDE,
  LOOP_ANCHOR_ID,
  PREBUILT_AGENTS,
  type AgentPillar,
  type PrebuiltAgent,
} from "../lib/marketing-copy";

/*
 * Prebuilt agents — the droids-style "ready to run" wall that replaced the old
 * loop · replayed ledger (redundant with HowItWorks + the hero lede). Each card
 * is category · name · description · connected-app logos · a waitlist CTA. The
 * three pillars beneath restate the product in one screen: an isolated machine
 * per run, operational memory that compounds, and proactive wake-on-event with
 * a replayable log. Keeps id={LOOP_ANCHOR_ID} so the hero "Meet the agents"
 * link, the footer "agents" link, nav, and llms.txt anchor all still resolve.
 */
export default function PrebuiltAgents() {
  return (
    <Section asChild className="site-section" data-testid="prebuilt-agents">
      <section id={LOOP_ANCHOR_ID} aria-label="Prebuilt agents, ready to run">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <SectionLabel className="mb-0">the fleet · ready to run</SectionLabel>
            <DisplayLG>{AGENTS_SECTION_HEADING}</DisplayLG>
            <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0 max-w-narrow">
              {AGENTS_SECTION_LEDE}
            </p>
          </div>

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {PREBUILT_AGENTS.map((agent) => (
              <AgentCard key={agent.id} agent={agent} />
            ))}
            <ComingSoonCard />
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            {AGENT_PILLARS.map((pillar) => (
              <PillarCard key={pillar.id} pillar={pillar} />
            ))}
          </div>
        </div>
      </section>
    </Section>
  );
}

function AgentCard({ agent }: { agent: PrebuiltAgent }) {
  return (
    <Card
      className="flex h-full flex-col gap-4"
      data-testid={`agent-card-${agent.id}`}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
          {agent.category}
        </span>
        {agent.comingSoon ? (
          <span
            className="rounded-sm border border-border px-2 py-0.5 font-mono text-label uppercase tracking-label text-text-subtle"
            data-testid={`agent-coming-soon-${agent.id}`}
          >
            Coming soon
          </span>
        ) : null}
      </div>
      <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
        {agent.name}
      </h3>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
        {agent.description}
      </p>

      <div className="mt-auto flex flex-col gap-4">
        <div
          className="flex flex-wrap items-center gap-2"
          data-testid={`agent-integrations-${agent.id}`}
        >
          {agent.integrations.map((integration) => (
            <span
              key={integration.label}
              className="inline-flex items-center gap-1.5 rounded-sm border border-border px-2 py-1"
            >
              <img
                src={integration.icon}
                alt=""
                aria-hidden="true"
                loading="lazy"
                decoding="async"
                className="size-4 shrink-0"
              />
              <span className="font-mono text-label text-text-muted">
                {integration.label}
              </span>
            </span>
          ))}
        </div>

        <Button
          asChild
          variant="secondary"
          className="min-h-11 w-full justify-center"
        >
          <a
            href={WAITLIST_URL}
            data-testid={`agent-cta-${agent.id}`}
            onClick={() =>
              trackSignupStarted({
                source: `agent_${agent.id}`,
                surface: "agents",
                mode: "humans",
              })
            }
          >
            {agent.comingSoon ? "Join the waitlist" : "Try it"}
          </a>
        </Button>
      </div>
    </Card>
  );
}

function ComingSoonCard() {
  return (
    <div
      className="flex h-full min-h-[12rem] flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border p-2xl text-center"
      data-testid="agent-card-coming-soon"
    >
      <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
        more
      </span>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
        More prebuilt agents are joining the fleet — coming soon.
      </p>
    </div>
  );
}

function PillarCard({ pillar }: { pillar: AgentPillar }) {
  return (
    <Card className="flex flex-col gap-2" data-testid={`agent-pillar-${pillar.id}`}>
      <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
        {pillar.eyebrow}
      </span>
      <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
        {pillar.title}
      </h3>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
        {pillar.description}
      </p>
    </Card>
  );
}
