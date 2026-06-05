import { describe, expect, it } from "vitest";
import { PROVIDER_GUIDANCE, guidanceFor } from "./provider-guidance";

const WEBHOOK = "https://api-dev.usezombie.com/v1/webhooks/zmb_test";

describe("PROVIDER_GUIDANCE", () => {
  describe("github", () => {
    it("renders the gh api command with vars substituted and events expanded", () => {
      const out = PROVIDER_GUIDANCE.github.command(
        { OWNER: "acme", REPO: "platform" },
        `${WEBHOOK}/github`,
        ["workflow_run", "push"],
      );
      expect(out).toBe(
        [
          `gh api -X POST repos/acme/platform/hooks`,
          `  -F "name=web"`,
          `  -F "events[]=workflow_run" -F "events[]=push"`,
          `  -F "config[url]=https://api-dev.usezombie.com/v1/webhooks/zmb_test/github"`,
          `  -F "config[content_type]=json"`,
        ].join(" \\\n"),
      );
    });

    it("formats the events label as a comma-joined sentence", () => {
      expect(PROVIDER_GUIDANCE.github.eventsLabel(["workflow_run", "push"])).toBe(
        "On workflow_run, push",
      );
      expect(PROVIDER_GUIDANCE.github.eventsLabel([])).toBe("On workflow_run");
    });

    it("targets the repo's hooks page", () => {
      expect(
        PROVIDER_GUIDANCE.github.webUiDeepLink({ OWNER: "acme", REPO: "platform" }),
      ).toBe("https://github.com/acme/platform/settings/hooks/new");
    });
  });

  describe("linear", () => {
    it("renders a graphql webhookCreate mutation with the team id and resources", () => {
      const out = PROVIDER_GUIDANCE.linear.command(
        { TEAM_ID: "ENG" },
        `${WEBHOOK}/linear`,
        ["Issue"],
      );
      expect(out).toContain(`teamId: \\"ENG\\"`);
      expect(out).toContain(`url: \\"${WEBHOOK}/linear\\"`);
      // GraphQL enum list — bare identifiers, no quotes. Quoted forms would
      // break the outer JSON payload boundary.
      expect(out).toContain(`resourceTypes: [Issue]`);
      expect(out).not.toContain(`resourceTypes: ["Issue"]`);
      expect(out).toContain(`curl -X POST https://api.linear.app/graphql`);
    });

    it("renders multiple resourceTypes as a bare-identifier list", () => {
      const out = PROVIDER_GUIDANCE.linear.command(
        { TEAM_ID: "ENG" },
        `${WEBHOOK}/linear`,
        ["Issue", "Comment"],
      );
      expect(out).toContain(`resourceTypes: [Issue, Comment]`);
    });

    it("produces a JSON-valid -d payload (no quote escaping pitfalls)", () => {
      const out = PROVIDER_GUIDANCE.linear.command(
        { TEAM_ID: "ENG" },
        `${WEBHOOK}/linear`,
        ["Issue", "Comment"],
      );
      // Extract the single-quoted -d payload and verify it parses as JSON.
      // A failure here was the original bug — JSON.stringify on the enum
      // list embedded literal `"` chars inside the outer `"..."` string.
      const match = out.match(/-d '(\{[\s\S]*\})'/);
      if (!match || match[1] === undefined) {
        throw new Error(`-d payload not found in: ${out}`);
      }
      const parsed = JSON.parse(match[1]);
      expect(typeof parsed.query).toBe("string");
      expect(parsed.query).toContain("resourceTypes: [Issue, Comment]");
    });

    it("labels multiple resources comma-joined and falls back to Issue when empty", () => {
      expect(PROVIDER_GUIDANCE.linear.eventsLabel(["Issue", "Comment"])).toBe(
        "On Issue, Comment",
      );
      expect(PROVIDER_GUIDANCE.linear.eventsLabel([])).toBe("On Issue");
    });

    it("deep-links to the Linear API settings", () => {
      expect(PROVIDER_GUIDANCE.linear.webUiDeepLink({})).toBe(
        "https://linear.app/settings/api",
      );
    });
  });

  describe("jira", () => {
    it("renders a curl against the workspace's webhooks endpoint", () => {
      const out = PROVIDER_GUIDANCE.jira.command(
        { WORKSPACE: "your-org" },
        `${WEBHOOK}/jira`,
        ["jira:issue_created"],
      );
      expect(out).toContain(
        `curl -X POST https://your-org.atlassian.net/rest/webhooks/1.0/webhook`,
      );
      expect(out).toContain(`"url":"${WEBHOOK}/jira"`);
      expect(out).toContain(`"events":["jira:issue_created"]`);
    });

    it("labels multiple events comma-joined and falls back to jira:issue_created when empty", () => {
      expect(
        PROVIDER_GUIDANCE.jira.eventsLabel([
          "jira:issue_created",
          "jira:issue_updated",
        ]),
      ).toBe("On jira:issue_created, jira:issue_updated");
      expect(PROVIDER_GUIDANCE.jira.eventsLabel([])).toBe("On jira:issue_created");
    });

    it("deep-links to the workspace's webhooks plugin page", () => {
      expect(PROVIDER_GUIDANCE.jira.webUiDeepLink({ WORKSPACE: "your-org" })).toBe(
        "https://your-org.atlassian.net/plugins/servlet/webhooks",
      );
    });
  });

  describe("grafana", () => {
    it("renders a curl against the stack's provisioning API", () => {
      const out = PROVIDER_GUIDANCE.grafana.command(
        { STACK_NAME: "ops-grafana" },
        `${WEBHOOK}/grafana`,
        ["alert"],
      );
      expect(out).toContain(
        `curl -X POST https://ops-grafana.grafana.net/api/v1/provisioning/contact-points`,
      );
      expect(out).toContain(`"settings":{"url":"${WEBHOOK}/grafana"}`);
    });

    it("labels multiple events comma-joined and falls back to alert when empty", () => {
      expect(PROVIDER_GUIDANCE.grafana.eventsLabel(["alert", "resolved"])).toBe(
        "On alert, resolved",
      );
      expect(PROVIDER_GUIDANCE.grafana.eventsLabel([])).toBe("On alert");
    });

    it("deep-links to the stack's alerting notifications page", () => {
      expect(
        PROVIDER_GUIDANCE.grafana.webUiDeepLink({ STACK_NAME: "ops-grafana" }),
      ).toBe("https://ops-grafana.grafana.net/alerting/notifications/new");
    });
  });

  describe("slack", () => {
    it("renders the numbered Events-API checklist with the webhook URL inline", () => {
      const out = PROVIDER_GUIDANCE.slack.command(
        {},
        `${WEBHOOK}/slack`,
        ["message.channels"],
      );
      expect(out).toContain(`https://api.slack.com/apps`);
      expect(out).toContain(`${WEBHOOK}/slack`);
      expect(out.split("\n").length).toBeGreaterThanOrEqual(5);
    });

    it("deep-links to slack apps registration", () => {
      expect(PROVIDER_GUIDANCE.slack.webUiDeepLink({})).toBe(
        "https://api.slack.com/apps?new_app=1",
      );
    });
  });

  describe("agentmail", () => {
    it("renders a curl against the inbox's webhooks collection", () => {
      const out = PROVIDER_GUIDANCE.agentmail.command(
        { INBOX: "ops@example.com" },
        `${WEBHOOK}/agentmail`,
        ["inbound"],
      );
      expect(out).toContain(
        `curl -X POST https://api.agentmail.to/v1/inboxes/ops@example.com/webhooks`,
      );
      expect(out).toContain(`"url":"${WEBHOOK}/agentmail"`);
      expect(out).toContain(`"events":["inbound"]`);
    });

    it("labels multiple events comma-joined and falls back to inbound when empty", () => {
      expect(
        PROVIDER_GUIDANCE.agentmail.eventsLabel(["inbound", "bounced"]),
      ).toBe("On inbound, bounced");
      expect(PROVIDER_GUIDANCE.agentmail.eventsLabel([])).toBe("On inbound");
    });

    it("deep-links to the inbox's webhooks page in the AgentMail console", () => {
      expect(
        PROVIDER_GUIDANCE.agentmail.webUiDeepLink({ INBOX: "ops@example.com" }),
      ).toBe("https://app.agentmail.to/inboxes/ops@example.com/webhooks");
    });
  });

  describe("missing variables", () => {
    it("renders an angle-bracket placeholder so the command stays copy-pasteable", () => {
      const out = PROVIDER_GUIDANCE.github.command({}, WEBHOOK, ["workflow_run"]);
      expect(out).toContain("repos/<OWNER>/<REPO>/hooks");
    });
  });

  describe("guidanceFor()", () => {
    it("returns the card for a known source", () => {
      expect(guidanceFor("github")).toBe(PROVIDER_GUIDANCE.github);
    });

    it("returns null for an unknown source so callers can fall back to CopyUrlCard", () => {
      expect(guidanceFor("weirdco")).toBeNull();
    });
  });
});
