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

const NATIVE_SETUP_SUMMARY = "Describe and review a job in workspace chat. Save creates a fleet.";

export const MARKETING_POSITIONING_SUMMARY =
  `Open-source software for running AI agents on your infrastructure or through the hosted service. A fleet checks the logs and code you allow and shows what it found. A separate fleet with write access can draft a fix. Your team reviews and ships it. ${NATIVE_SETUP_SUMMARY} The homepage uses sample data.`;

export const LLMS_FULL_INTRO =
  `${PRODUCT_NAME} runs AI agents on a message, a schedule, or an event. Choose the hosted model or use your own model key. ${NATIVE_SETUP_SUMMARY} In the homepage example, Leah reports a frozen chat in Slack. Scout checks Grafana and finds slow replies and dropped connections. A repair fleet with write access opens a draft pull request. Leah reviews and deploys it. Scout checks reply times again, and Mira shares the result in Slack. The demo uses sample data.`;

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
    `- [The fleet](${root}/#${LOOP_ANCHOR_ID}): follow a Slack ticket from a frozen chat to faster replies`,
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
    PRICING_COPY.runtime,
    PRICING_COPY.note,
    "",
    "## Links",
    `- Docs: ${inputs.docsUrl}`,
    `- Source: ${inputs.githubUrl}`,
    "",
  ].join("\n");
}
