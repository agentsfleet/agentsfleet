// No Vite-only syntax here (?url, CSS imports, import.meta.env):
// tests/e2e/smoke.spec.ts loads this through Playwright's transform and
// scripts/prebuild.mjs through Bun, and neither runs Vite plugins.
import {
  LOOP_ANCHOR_ID,
  LOOP_STEPS,
  PILLAR_TOKENS,
  PRODUCT_NAME,
  PRICING_COPY,
  SOURCE_CATEGORIES,
} from "./marketing-copy";

const NATIVE_SETUP_SUMMARY = "Describe and review a job in workspace chat. Save creates a fleet. Publishing to the Library is a separate action.";

export const MARKETING_POSITIONING_SUMMARY =
  `Open-source runtime for AI agents that wake on production events: they investigate with your logs, metrics, and code, produce an evidence-backed result, and work within configured access. An investigator does not change code; a separately configured repairer can open a draft pull request. Humans review and merge. ${NATIVE_SETUP_SUMMARY} The homepage run uses sample data.`;

export const LLMS_FULL_INTRO =
  `${PRODUCT_NAME} is an open-source runtime for AI agents that wake on events from production and your repositories. Each agent starts on one — an incident, a failed deploy, a pull request — reads only the sources you allow, and returns an evidence-backed result. Runs use the platform’s model or your own model key. Some runs end with diagnosis. The incident responder reads Grafana Loki and GitHub and can report to configured Slack and Jira destinations. A separately configured repairer needs an approved write grant to open a draft pull request. It never merges or deploys. ${NATIVE_SETUP_SUMMARY} The homepage conversation is an illustrative example using sample data; it does not connect tools or start real work.`;

export type LlmsTextInputs = {
  docsUrl: string;
  githubUrl: string;
  siteUrl: string;
};

export function buildLlmsIndexText({
  docsUrl,
  githubUrl,
  siteUrl,
}: LlmsTextInputs): string {
  const root = siteUrl.replace(/\/$/, "");
  return [
    `# ${PRODUCT_NAME}`,
    "",
    `> ${MARKETING_POSITIONING_SUMMARY}`,
    "",
    "## Product",
    `- [The fleet](${root}/#${LOOP_ANCHOR_ID}): configure a specialist job; explore an illustrative incident run`,
    `- [Getting started and pricing](${root}/#pricing): ${PRICING_COPY.status}. ${PRICING_COPY.note}`,
    "",
    "## Resources",
    `- [Docs](${docsUrl})`,
    "- [OpenAPI](/openapi.json)",
    `- [Source](${githubUrl})`,
    "",
  ].join("\n");
}

export function buildLlmsFullText(inputs: LlmsTextInputs): string {
  const sourceLines = SOURCE_CATEGORIES.map((category) => {
    return `- ${category.label}: ${category.examples.join(", ")}`;
  });
  const loopLines = LOOP_STEPS.map((step) => {
    return `- ${step.number}. ${step.title}: ${step.description}`;
  });

  return [
    `# ${PRODUCT_NAME} full context`,
    "",
    LLMS_FULL_INTRO,
    "",
    "## Pillars",
    ...PILLAR_TOKENS.map((token) => `- ${token}`),
    "",
    "## Loop",
    ...loopLines,
    "",
    "## Sources",
    ...sourceLines,
    "",
    "## Pricing",
    PRICING_COPY.status,
    PRICING_COPY.runtime,
    PRICING_COPY.models,
    PRICING_COPY.note,
    "",
    "## Links",
    `- Docs: ${inputs.docsUrl}`,
    `- Source: ${inputs.githubUrl}`,
    "",
  ].join("\n");
}
