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

export const MARKETING_POSITIONING_SUMMARY =
  "Open-source runtime for AI agents that wake on production events: they investigate with your logs, metrics, and code, produce an evidence-backed result, and require human approval before configured repair work can merge or ship.";

export const LLMS_FULL_INTRO =
  "agentsfleet is an open-source runtime for AI agents that wake on production events. Each agent starts on an event — a pull request, an incident, a deploy — reads only the sources you allow-list, and returns an evidence-backed result, on the platform’s model or a key you bring. Some runs end with diagnosis; configured repair work waits for human approval before anything merges or ships.";

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
    `- [The fleet](${root}/#${LOOP_ANCHOR_ID}): prebuilt fleets that wake on your events`,
    `- [Early access and pricing](${root}/#pricing): ${PRICING_COPY.status}. ${PRICING_COPY.note}`,
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
