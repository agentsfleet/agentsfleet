import { describe, expect, it } from "vitest";
import { DOCS_URL, GITHUB_URL, MARKETING_SITE_URL } from "../config";
import { buildLlmsFullText, buildLlmsIndexText } from "./llms-text";
import { FIRST_MESSAGE_FLOW, LOOP_ANCHOR_ID, PRICING_COPY } from "./marketing-copy";

const inputs = { docsUrl: DOCS_URL, githubUrl: GITHUB_URL, siteUrl: MARKETING_SITE_URL } as const;

describe("llms text builders", () => {
  it("renders an index of the product and developer resources", () => {
    const body = buildLlmsIndexText(inputs);
    expect(body).toMatch(/^# agentsfleet\n\n> /);
    expect(body).toContain("## Product");
    expect(body).toContain(`https://www.agentsfleet.net/#${LOOP_ANCHOR_ID}`);
    expect(body).toContain("https://www.agentsfleet.net/examples");
    expect(body).toContain(`[Docs](${DOCS_URL})`);
    expect(body).toContain("[OpenAPI](/openapi.json)");
    expect(body).toContain(`[Source](${GITHUB_URL})`);
  });

  it("trims a trailing slash from the site URL when building anchors", () => {
    const body = buildLlmsIndexText({ ...inputs, siteUrl: "https://example.test/" });
    expect(body).toContain(`https://example.test/#${LOOP_ANCHOR_ID}`);
    expect(body).toContain("https://example.test/#pricing");
  });

  it("explains first-message creation and in-chat access approval", () => {
    for (const body of [buildLlmsIndexText(inputs), buildLlmsFullText(inputs)]) {
      expect(body).toContain("Your first message creates the fleet.");
      expect(body).toContain("You can approve its request once or deny it.");
      expect(body).not.toMatch(/save creates|approval rules|fleet budget/i);
    }
  });

  it("covers recurring work, longer tasks, draft pull requests, and pricing in the full text", () => {
    const body = buildLlmsFullText(inputs);
    expect(body).toContain("a GitHub event, a Slack mention, a webhook, or a schedule");
    expect(body).toContain("start a task that takes several runs");
    expect(body).toContain("a draft pull request for your team to review");
    for (const step of FIRST_MESSAGE_FLOW) expect(body).toContain(step.title);
    expect(body).toContain(PRICING_COPY.note);
    expect(body).not.toMatch(/starter credit|zero markup|unlimited/i);
  });
});
