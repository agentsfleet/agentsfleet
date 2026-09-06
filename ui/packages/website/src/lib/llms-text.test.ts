import { describe, expect, it } from "vitest";
import { DOCS_URL, GITHUB_URL, MARKETING_SITE_URL } from "../config";
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
  PRICING_COPY,
} from "./marketing-copy";

const inputs = {
  docsUrl: DOCS_URL,
  githubUrl: GITHUB_URL,
  siteUrl: MARKETING_SITE_URL,
} as const;

describe("llms text builders", () => {
  it("should render llms.txt in the convention shape", () => {
    const body = buildLlmsIndexText(inputs);
    expect(body).toMatch(/^# agentsfleet\n\n> /);
    expect(body).toContain(MARKETING_POSITIONING_SUMMARY);
    expect(body).toContain("## Product");
    expect(body).toContain("## Resources");
    expect(body).toContain(`https://agentsfleet.net/#${LOOP_ANCHOR_ID}`);
    expect(body).toContain(PRICING_COPY.note);
    expect(body).toContain("produce an evidence-backed result");
    expect(body).not.toContain("Each one opens a fix");
    expect(body).not.toMatch(/\$\d|starter credit|zero markup/i);
    expect(body).toContain(`[Docs](${DOCS_URL})`);
    expect(body).toContain("[OpenAPI](/openapi.json)");
    expect(body).toContain(`[Source](${GITHUB_URL})`);
    expect(body).not.toContain("curl -fsSL");
  });

  it("should trim a trailing slash from the site URL when building anchors", () => {
    const body = buildLlmsIndexText({
      ...inputs,
      siteUrl: "https://example.test/",
    });
    expect(body).toContain(`https://example.test/#${LOOP_ANCHOR_ID}`);
    expect(body).toContain("https://example.test/#pricing");
    expect(body).not.toContain("https://example.test//#");
  });

  it("should render llms-full.txt with pillars, loop, sources, and links", () => {
    const body = buildLlmsFullText(inputs);
    expect(body).toContain("Some runs end with diagnosis");
    expect(body).not.toContain("Each one opens a fix");
    for (const token of PILLAR_TOKENS) {
      expect(body).toContain(`- ${token}`);
    }
    for (const step of LOOP_STEPS) {
      expect(body).toContain(`${step.number}. ${step.title}`);
    }
    for (const category of SOURCE_CATEGORIES) {
      expect(body).toContain(`- ${category.label}: ${category.examples.join(", ")}`);
    }
    expect(body).toContain(PRICING_COPY.note);
    expect(body).not.toMatch(/\$\d|starter credit|zero markup/i);
    expect(body).not.toContain("curl -fsSL");
  });
});
