import {
  LOOP_ANCHOR_ID,
  LOOP_STEPS,
  PILLAR_TOKENS,
  PRODUCT_NAME,
  SOURCE_CATEGORIES,
} from "./marketing-copy";

export const MARKETING_POSITIONING_SUMMARY =
  "Prebuilt AI teammates for recurring engineering work: they wake on your events, open a scenario-backed fix, and hold at human approval before merge or deploy.";

export const LLMS_FULL_INTRO =
  "agentsfleet is a fleet of prebuilt AI teammates for recurring engineering work. Each one wakes on an event — a pull request, an incident, a deploy — reads only the sources you allow-list, opens a fix with the evidence attached, and waits for human approval before anything merges or ships.";

export type LlmsTextInputs = {
  docsUrl: string;
  githubUrl: string;
  installCommand: string;
  siteUrl: string;
  runRatePerSecond: string;
  starterCredit: string;
  eventRate: string;
};

export function buildLlmsIndexText({
  docsUrl,
  githubUrl,
  installCommand,
  siteUrl,
  runRatePerSecond,
  starterCredit,
  eventRate,
}: LlmsTextInputs): string {
  const root = siteUrl.replace(/\/$/, "");
  return [
    `# ${PRODUCT_NAME}`,
    "",
    `> ${MARKETING_POSITIONING_SUMMARY}`,
    "",
    "## Product",
    `- [The fleet](${root}/#${LOOP_ANCHOR_ID}): prebuilt fleets, ready to run`,
    `- [Pricing](${root}/#pricing): ${runRatePerSecond} run, ${starterCredit} starter credit, events ${eventRate}`,
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
    `- Run rate: ${inputs.runRatePerSecond}`,
    `- Starter credit: ${inputs.starterCredit}`,
    `- Events: ${inputs.eventRate}`,
    "",
    "## Links",
    `- Docs: ${inputs.docsUrl}`,
    `- Source: ${inputs.githubUrl}`,
    `- Install: ${inputs.installCommand}`,
    "",
  ].join("\n");
}
