import { FIRST_MESSAGE_FLOW, LOOP_ANCHOR_ID, PRICING_COPY, PRODUCT_NAME } from "./marketing-copy";

type LlmsTextInputs = {
  docsUrl: string;
  githubUrl: string;
  siteUrl: string;
};

export const MARKETING_POSITIONING_SUMMARY =
  `agentsfleet is open-source software for engineering work. A fleet is an AI agent. ${FIRST_MESSAGE_FLOW[0].description} ${FIRST_MESSAGE_FLOW[1].description}`;

export const LLMS_FULL_INTRO =
  `${MARKETING_POSITIONING_SUMMARY} Ask a fleet to wake on a GitHub event, a Slack mention, a webhook, or a schedule. You can ask it to start a task that takes several runs. With approved write access, it can open a draft pull request for your team to review.`;

export function buildLlmsIndexText({ docsUrl, githubUrl, siteUrl }: LlmsTextInputs): string {
  const root = siteUrl.replace(/\/$/, "");
  return [
    `# ${PRODUCT_NAME}`,
    "",
    `> ${MARKETING_POSITIONING_SUMMARY}`,
    "",
    "## Product",
    `- [How fleets work](${root}/#${LOOP_ANCHOR_ID})`,
    `- [Example conversations](${root}/examples)`,
    `- [Early access pricing](${root}/#pricing)`,
    "",
    "## Resources",
    `- [Docs](${docsUrl})`,
    "- [OpenAPI](/openapi.json)",
    `- [Source](${githubUrl})`,
    "",
  ].join("\n");
}

export function buildLlmsFullText(inputs: LlmsTextInputs): string {
  return [
    `# ${PRODUCT_NAME} full context`,
    "",
    LLMS_FULL_INTRO,
    "",
    "## How fleets work",
    ...FIRST_MESSAGE_FLOW.map(({ title, description }) => `- **${title}.** ${description}`),
    "",
    "## Model and source code",
    "Choose a hosted model or bring your own model key. The source code is open under the MIT license.",
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
