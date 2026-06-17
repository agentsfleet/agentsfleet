import {
  Badge,
  Card,
  DisplayLG,
  LogLine,
  LogToken,
  Section,
  SectionLabel,
  Terminal,
} from "@agentsfleet/design-system";
import {
  LEDGER_LINES,
  LOOP_ANCHOR_ID,
  SOURCE_CATEGORIES,
  type LedgerLine,
  type SourceCategory,
} from "../lib/marketing-copy";

export default function PipelineDiagram() {
  return (
    <Section asChild className="site-section" data-testid="pipeline-diagram">
      <section
        id={LOOP_ANCHOR_ID}
        aria-label="Compounding operational knowledge loop"
      >
        <div className="wrap grid grid-cols-1 gap-8 lg:grid-cols-3">
          <div className="flex flex-col gap-3 lg:col-span-1">
            <SectionLabel className="mb-0">the loop · replayed</SectionLabel>
            <DisplayLG>
              Watch one incident become prevention.
            </DisplayLG>
            <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0">
              Not a chat transcript. An evidence ledger: every step timestamped,
              every action gated, every line replayable.
            </p>
            <SourceStrip />
          </div>

          <Terminal
            label="agentsfleet · run a1f9 · core.agent_events"
            className="lg:col-span-2"
            data-testid="pipeline-ledger"
          >
            {LEDGER_LINES.map((line) => (
              <LedgerLogLine key={line.id} line={line} />
            ))}
          </Terminal>
        </div>
      </section>
    </Section>
  );
}

function SourceStrip() {
  return (
    <div
      className="grid grid-cols-1 gap-3 sm:grid-cols-2"
      data-testid="pipeline-source-strip"
    >
      {SOURCE_CATEGORIES.map((category) => (
        <SourceCard key={category.id} category={category} />
      ))}
    </div>
  );
}

function SourceCard({ category }: { category: SourceCategory }) {
  return (
    <Card
      className="flex min-w-0 flex-col gap-3 p-lg"
      data-testid={`pipeline-source-${category.id}`}
    >
      <div className="flex items-center gap-2">
        <img
          src={category.icon}
          alt=""
          aria-hidden="true"
          loading="lazy"
          decoding="async"
          className="size-5 shrink-0"
        />
        <Badge variant="default">{category.label}</Badge>
      </div>
      <p className="font-sans text-body-sm leading-body-sm text-text-muted m-0">
        {category.examples.join(" · ")}
      </p>
    </Card>
  );
}

function LedgerLogLine({ line }: { line: LedgerLine }) {
  const isApproval = line.id === "approval";
  return (
    <LogLine
      severity={line.severity}
      className="flex gap-3 whitespace-pre-wrap"
      data-testid={`pipeline-ledger-line-${line.id}`}
    >
      <LogToken severity="debug" className="shrink-0">
        {line.timestamp}
      </LogToken>
      {line.tag ? (
        <LogToken severity={line.tagSeverity} className="shrink-0">
          {line.tag}
        </LogToken>
      ) : null}
      {isApproval ? (
        <span className="text-text" data-testid="pipeline-human-gate">
          <LogToken severity="debug" className="mr-2">
            hold
          </LogToken>
          {line.message}
        </span>
      ) : (
        <span>{line.message}</span>
      )}
    </LogLine>
  );
}
