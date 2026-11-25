export const JOB_ID = { incident: "incident", tests: "tests", agent: "agent" } as const;
export type JobId = (typeof JOB_ID)[keyof typeof JOB_ID];
export type Evidence = { source: string; label: string; detail: string; excerpt: string; url?: string };
export type ExampleJob = {
  id: JobId; name: string; label: string; request: string; title: string;
  firstReply: string; approvedAccess: readonly string[]; schedule: string; check: string;
  activity: string; investigation: string; boundary: string; finding: string; proposal: string;
  code: readonly string[]; evidence: readonly Evidence[]; report: string;
};

const READ_ACCESS = "GitHub access covers reading. I can open a draft pull request if you later approve write access.";
export const EXAMPLE_REPOSITORY_URL = "https://github.com/agentsfleet/linkwarden";
const SOURCE_ROOT = `${EXAMPLE_REPOSITORY_URL}/blob/952ac4540657cae3a67c3ca59433899d2fda8374`;

export const EXAMPLE_JOBS = [
  {
    id: JOB_ID.incident, name: "Linkwarden watch", label: "Investigate an incident",
    request: "Check agentsfleet/linkwarden every 15 minutes for failed page archives. Read the worker logs in Grafana and tell me what needs fixing.",
    title: "Why are Linkwarden archives timing out?",
    firstReply: "I’ll check every 15 minutes and report here. I need access to Grafana and the Linkwarden repository.",
    approvedAccess: ["GitHub · agentsfleet/linkwarden · Read", "Grafana"],
    schedule: "Every 15 minutes",
    check: "Scheduled check started",
    activity: "Reading the worker logs",
    investigation: "The worker stops preserving a page after five minutes. I’m checking its timeout setting against the repository and the supplied configuration.",
    boundary: READ_ACCESS,
    finding: "BROWSER_TIMEOUT was left unset. Linkwarden uses a five-minute default, which is too short for this example’s slow page. The setting is measured in minutes.",
    proposal: "Add BROWSER_TIMEOUT=10 to the deployment environment, restart the worker, and retry the same page. Verify the result before keeping the higher limit.",
    code: ["  # Deployment environment · example override", "+ BROWSER_TIMEOUT=10"],
    evidence: [
      { source: "Grafana Loki", label: "Sample worker log", detail: "Example run · Linkwarden worker", excerpt: "Failed Link: [sample page]\nReason: Browser has been open for more than 5 minutes." },
      { source: "GitHub", label: "The real timeout default", detail: "agentsfleet/linkwarden · archiveHandler.ts", excerpt: "const BROWSER_TIMEOUT = Number(process.env.BROWSER_TIMEOUT) || 5;\nTimer duration: BROWSER_TIMEOUT × 60000 milliseconds", url: `${SOURCE_ROOT}/apps/worker/lib/archiveHandler.ts#L23` },
      { source: "GitHub", label: "The documented setting", detail: "agentsfleet/linkwarden · .env.sample", excerpt: "BROWSER_TIMEOUT=\nThe example deployment left this optional setting unset.", url: `${SOURCE_ROOT}/.env.sample` },
    ],
    report: "The worker stopped after five minutes because it used the default timeout. Try BROWSER_TIMEOUT=10, restart the worker, and retry the page to confirm.",
  },
  {
    id: JOB_ID.tests, name: "Regression watch", label: "Catch test regressions",
    request: "Check the archive URL tests in agentsfleet/linkwarden every 15 minutes. Tell me about failures before release.",
    title: "Catch a Linkwarden archive test regression",
    firstReply: "I’ll check the archive tests every 15 minutes and report failures here. I need access to the Linkwarden repository.",
    approvedAccess: ["GitHub · agentsfleet/linkwarden · Read"],
    schedule: "Every 15 minutes",
    check: "Scheduled check started",
    activity: "Comparing test results",
    investigation: "In this example, the signed archive URL test fails. I’m checking the expected content domain against the generated link.",
    boundary: READ_ACCESS,
    finding: "The archive URL points to the wrong host. Linkwarden’s existing test expects the configured content domain to be used.",
    proposal: "Check NEXT_PUBLIC_USER_CONTENT_DOMAIN and the URL-building change, then rerun the signed archive URL test before release.",
    code: ["  # Test environment · sample content domain", "+ NEXT_PUBLIC_USER_CONTENT_DOMAIN=https://content.example.com"],
    evidence: [
      { source: "Test output", label: "Sample failing assertion", detail: "Illustrative regression · archive URL host", excerpt: "expected origin: https://content.example.com\nreceived origin: http://localhost:3000" },
      { source: "GitHub", label: "The existing repository test", detail: "createPreservedFormatUrl.test.ts", excerpt: "Test: creates a signed preserved-format URL\nExpected origin: the configured content domain", url: `${SOURCE_ROOT}/apps/web/lib/api/preserved/createPreservedFormatUrl.test.ts` },
    ],
    report: "The sample test caught an archive URL using the wrong host. Check the content-domain setting and URL builder, then rerun the test. Repository unchanged.",
  },
  {
    id: JOB_ID.agent, name: "Release brief", label: "Automate recurring work",
    request: "Write a release brief for agentsfleet/linkwarden every Monday at 9 am UTC. Flag anything that needs follow-up.",
    title: "An agent for your weekly release brief",
    firstReply: "I’ll post a brief here every Monday. I need access to read what merged in the Linkwarden repository.",
    approvedAccess: ["GitHub · agentsfleet/linkwarden · Read"],
    schedule: "Mondays at 9 am UTC",
    check: "Weekly brief started",
    activity: "Reading this week’s changes",
    investigation: "I found three merged pull requests. I’m reading their descriptions and test results for the release brief.",
    boundary: "I’ll keep the brief and its links in this chat.",
    finding: "Three changes are ready to summarize. A database migration has a rollback question that still needs an owner.",
    proposal: "Include the three changes in the brief and ask the migration owner to confirm the rollback steps before release.",
    code: ["  Weekly release brief", "+ 3 merged changes with links", "+ Follow up: confirm migration rollback steps"],
    evidence: [
      { source: "GitHub", label: "Sample weekly changes", detail: "agentsfleet/linkwarden · illustrative brief", excerpt: "Archive reliability improvements\nCollection sharing improvements\nDatabase migration" },
      { source: "GitHub", label: "Sample release follow-up", detail: "Illustrative migration notes", excerpt: "Migration: saved-link records\nRollback notes: awaiting owner confirmation" },
    ],
    report: "Your weekly brief includes three merged changes and their source links. Confirm the migration rollback steps before release.",
  },
] as const satisfies readonly ExampleJob[];

export const DEFAULT_JOB = EXAMPLE_JOBS[0];
export const REQUEST_MAX_LENGTH = 1000;
export const EXAMPLE_ANCHOR = "how-it-works";
export const JOBS_ANCHOR = "operational-loop";
