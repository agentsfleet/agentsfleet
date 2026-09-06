import {
  LOOP_ANCHOR_ID,
  LOOP_STEPS,
  PILLAR_TOKENS,
  PRODUCT_NAME,
  PRICING_COPY,
  SOURCE_CATEGORIES,
} from "./marketing-copy";

export const MARKETING_POSITIONING_SUMMARY =
  "Prebuilt AI teammates for recurring engineering work: they wake on your events, produce an evidence-backed result, and require human approval before configured repair work can merge or ship.";

export const LLMS_FULL_INTRO =
  "agentsfleet is a fleet of prebuilt AI teammates for recurring engineering work. Each one wakes on an event — a pull request, an incident, a deploy — reads only the sources you allow-list, and returns an evidence-backed result. Some runs end with diagnosis; configured repair work waits for human approval before anything merges or ships.";

export type LlmsTextInputs = {
  docsUrl: string;
  githubUrl: string;
  installCommand: string;
  siteUrl: string;
};

export function buildLlmsIndexText({
  docsUrl,
  githubUrl,
  installCommand,
  siteUrl,
}: LlmsTextInputs): string {
  const root = siteUrl.replace(/\/$/, "");
  return [
    `# ${PRODUCT_NAME}`,
    "",
    `> ${MARKETING_POSITIONING_SUMMARY}`,
    "",
    "## Product",
    `- [The fleet](${root}/#${LOOP_ANCHOR_ID}): prebuilt fleets, ready to run`,
    `- [Early access and pricing](${root}/#pricing): ${PRICING_COPY.status}. ${PRICING_COPY.note}`,
    "",
    "## Resources",
    `- [Docs](${docsUrl})`,
    "- [OpenAPI](/openapi.json)",
    `- [Source](${githubUrl})`,
    `- Install: \`${installCommand}\``,
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
    `- Install: ${inputs.installCommand}`,
    "",
  ].join("\n");
}
