// Per-provider data table consumed by GuidedTriggerCard. One entry per
// known webhook source declared in `TRIGGER.md`. Pure presentational
// strings — no fetching. Unknown sources fall back to CopyUrlCard in
// TriggerPanel.
//
// `command` takes the trigger's declared `events` as a third arg so the
// rendered shell snippet reflects what the user actually subscribes to.
// Without it the snapshot tests would have no way to vary events per case.

export type Source =
  | "github"
  | "linear"
  | "jira"
  | "grafana"
  | "slack"
  | "agentmail";

export type GuidanceVariable = {
  name: string;
  example: string;
  required: boolean;
};

export type GuidanceCard = {
  title: string;
  eventsLabel: (events: readonly string[]) => string;
  command: (
    vars: Record<string, string>,
    webhookUrl: string,
    events: readonly string[],
  ) => string;
  webUiDeepLink: (vars: Record<string, string>) => string;
  variables: readonly GuidanceVariable[];
};

const v = (name: string, example: string, required = true): GuidanceVariable => ({
  name,
  example,
  required,
});

const orPlaceholder = (vars: Record<string, string>, key: string, example: string) =>
  vars[key]?.trim() ? vars[key] : `<${example}>`;

const eventList = (events: readonly string[], fallback: string): readonly string[] =>
  events.length > 0 ? events : [fallback];

const joinEventsLabel = (events: readonly string[], fallback: string): string => {
  const list = eventList(events, fallback);
  return list.length === 1 ? `On ${list[0]}` : `On ${list.join(", ")}`;
};

export const PROVIDER_GUIDANCE: Record<Source, GuidanceCard> = {
  github: {
    title: "GitHub",
    eventsLabel: (events) => joinEventsLabel(events, "workflow_run"),
    command: (vars, webhookUrl, events) => {
      const owner = orPlaceholder(vars, "OWNER", "OWNER");
      const repo = orPlaceholder(vars, "REPO", "REPO");
      const evts = eventList(events, "workflow_run")
        .map((e) => `-F "events[]=${e}"`)
        .join(" ");
      return [
        `gh api -X POST repos/${owner}/${repo}/hooks`,
        `  -F "name=web"`,
        `  ${evts}`,
        `  -F "config[url]=${webhookUrl}"`,
        `  -F "config[content_type]=json"`,
      ].join(" \\\n");
    },
    webUiDeepLink: (vars) => {
      const owner = orPlaceholder(vars, "OWNER", "OWNER");
      const repo = orPlaceholder(vars, "REPO", "REPO");
      return `https://github.com/${owner}/${repo}/settings/hooks/new`;
    },
    variables: [v("OWNER", "acme"), v("REPO", "platform")],
  },

  linear: {
    title: "Linear",
    eventsLabel: (events) => joinEventsLabel(events, "Issue"),
    command: (vars, webhookUrl, events) => {
      const teamId = orPlaceholder(vars, "TEAM_ID", "TEAM_ID");
      const resources = JSON.stringify(eventList(events, "Issue"));
      return [
        `curl -X POST https://api.linear.app/graphql \\`,
        `  -H "Authorization: $LINEAR_API_KEY" \\`,
        `  -H "Content-Type: application/json" \\`,
        `  -d '{"query":"mutation { webhookCreate(input: { url: \\"${webhookUrl}\\", teamId: \\"${teamId}\\", resourceTypes: ${resources} }) { success } }"}'`,
      ].join("\n");
    },
    webUiDeepLink: () => `https://linear.app/settings/api`,
    variables: [v("TEAM_ID", "ENG")],
  },

  jira: {
    title: "Jira",
    eventsLabel: (events) => joinEventsLabel(events, "jira:issue_created"),
    command: (vars, webhookUrl, events) => {
      const workspace = orPlaceholder(vars, "WORKSPACE", "WORKSPACE");
      const evts = JSON.stringify(eventList(events, "jira:issue_created"));
      return [
        `curl -X POST https://${workspace}.atlassian.net/rest/webhooks/1.0/webhook \\`,
        `  -u "$JIRA_USER:$JIRA_API_TOKEN" \\`,
        `  -H "Content-Type: application/json" \\`,
        `  -d '{"name":"usezombie","url":"${webhookUrl}","events":${evts}}'`,
      ].join("\n");
    },
    webUiDeepLink: (vars) => {
      const workspace = orPlaceholder(vars, "WORKSPACE", "WORKSPACE");
      return `https://${workspace}.atlassian.net/plugins/servlet/webhooks`;
    },
    variables: [v("WORKSPACE", "your-org")],
  },

  grafana: {
    title: "Grafana",
    eventsLabel: (events) => joinEventsLabel(events, "alert"),
    command: (vars, webhookUrl) => {
      const stack = orPlaceholder(vars, "STACK_NAME", "STACK_NAME");
      return [
        `curl -X POST https://${stack}.grafana.net/api/v1/provisioning/contact-points \\`,
        `  -H "Authorization: Bearer $GRAFANA_API_TOKEN" \\`,
        `  -H "Content-Type: application/json" \\`,
        `  -d '{"name":"usezombie","type":"webhook","settings":{"url":"${webhookUrl}"}}'`,
      ].join("\n");
    },
    webUiDeepLink: (vars) => {
      const stack = orPlaceholder(vars, "STACK_NAME", "STACK_NAME");
      return `https://${stack}.grafana.net/alerting/notifications/new`;
    },
    variables: [v("STACK_NAME", "your-org-grafana")],
  },

  slack: {
    title: "Slack",
    eventsLabel: (events) => joinEventsLabel(events, "message"),
    // Slack has no public CLI for installing an Events-API app, so the
    // "command" here is a numbered checklist the user follows in the
    // web UI — pinned as a string so the snapshot test treats it the
    // same as the other entries.
    command: (_, webhookUrl) =>
      [
        `# Slack Events API — set up in the web UI:`,
        `# 1. Visit https://api.slack.com/apps and create a new app.`,
        `# 2. Under "Event Subscriptions", enable events.`,
        `# 3. Paste the Request URL:`,
        `#    ${webhookUrl}`,
        `# 4. Subscribe to bot events (message.channels, app_mention, ...).`,
        `# 5. Install the app into your workspace.`,
      ].join("\n"),
    webUiDeepLink: () => `https://api.slack.com/apps?new_app=1`,
    variables: [v("WORKSPACE", "your-org")],
  },

  agentmail: {
    title: "AgentMail",
    eventsLabel: (events) => joinEventsLabel(events, "inbound"),
    command: (vars, webhookUrl, events) => {
      const inbox = orPlaceholder(vars, "INBOX", "INBOX");
      const evts = JSON.stringify(eventList(events, "inbound"));
      return [
        `curl -X POST https://api.agentmail.to/v1/inboxes/${inbox}/webhooks \\`,
        `  -H "Authorization: Bearer $AGENTMAIL_API_KEY" \\`,
        `  -H "Content-Type: application/json" \\`,
        `  -d '{"url":"${webhookUrl}","events":${evts}}'`,
      ].join("\n");
    },
    webUiDeepLink: (vars) => {
      const inbox = orPlaceholder(vars, "INBOX", "INBOX");
      return `https://app.agentmail.to/inboxes/${inbox}/webhooks`;
    },
    variables: [v("INBOX", "ops@yourdomain.com")],
  },
};

export function guidanceFor(source: string): GuidanceCard | null {
  return (PROVIDER_GUIDANCE as Record<string, GuidanceCard>)[source] ?? null;
}
