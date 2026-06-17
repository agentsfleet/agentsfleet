import { describe, expect, it } from "vitest";
import { INSTALL_COMMAND, DOCS_URL, GITHUB_URL } from "../config";
import {
  buildLlmsFullText,
  buildLlmsIndexText,
  MARKETING_POSITIONING_SUMMARY,
} from "./llms-text";
import {
  LOOP_ANCHOR_ID,
  LOOP_STEPS,
  PILLAR_TOKENS,
  SOURCE_CATEGORIES,
} from "./marketing-copy";
import { RATES_DISPLAY } from "./rates";

const inputs = {
  docsUrl: DOCS_URL,
  githubUrl: GITHUB_URL,
  installCommand: INSTALL_COMMAND,
  runRatePerSecond: RATES_DISPLAY.RUN_RATE_PER_SEC,
  starterCredit: RATES_DISPLAY.STARTER_CREDIT,
  eventRate: RATES_DISPLAY.EVENT_RATE,
} as const;

describe("llms text builders", () => {
  it("should render llms.txt in the convention shape", () => {
    const body = buildLlmsIndexText(inputs);
    expect(body).toMatch(/^# agentsfleet\n\n> /);
    expect(body).toContain(MARKETING_POSITIONING_SUMMARY);
    expect(body).toContain("## Product");
    expect(body).toContain("## Resources");
    expect(body).toContain(`https://agentsfleet.net/#${LOOP_ANCHOR_ID}`);
    expect(body).toContain(RATES_DISPLAY.RUN_RATE_PER_SEC);
    expect(body).toContain(RATES_DISPLAY.STARTER_CREDIT);
    expect(body).toContain(RATES_DISPLAY.EVENT_RATE);
    expect(body).toContain(`[Docs](${DOCS_URL})`);
    expect(body).toContain("[OpenAPI](/openapi.json)");
    expect(body).toContain(`[Source](${GITHUB_URL})`);
    expect(body).toContain(`Install: \`${INSTALL_COMMAND}\``);
  });

  it("should render llms-full.txt with pillars, loop, sources, and links", () => {
    const body = buildLlmsFullText(inputs);
    for (const token of PILLAR_TOKENS) {
      expect(body).toContain(`- ${token}`);
    }
    for (const step of LOOP_STEPS) {
      expect(body).toContain(`${step.number}. ${step.title}`);
    }
    for (const category of SOURCE_CATEGORIES) {
      expect(body).toContain(`- ${category.label}: ${category.examples.join(", ")}`);
    }
    expect(body).toContain(`- Run rate: ${RATES_DISPLAY.RUN_RATE_PER_SEC}`);
    expect(body).toContain(`- Install: ${INSTALL_COMMAND}`);
  });
});
