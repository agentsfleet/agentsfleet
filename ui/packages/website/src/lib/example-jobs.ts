export const JOB_ID = { incident: "incident", tests: "tests", agent: "agent" } as const;
export type JobId = (typeof JOB_ID)[keyof typeof JOB_ID];
export type Evidence = { source: string; label: string; detail: string; excerpt: string; url?: string };
export type ExampleJob = {
  id: JobId; name: string; label: string; request: string; title: string;
  question: string; setupReply: string; setupReview: string; check: string;
  activity: string; investigation: string; boundary: string; finding: string; proposal: string;
  code: readonly string[]; evidence: readonly Evidence[]; report: string;
};

const READ_ACCESS = "This fleet can read your repository and explain the fix. Changing code needs a separate fleet with write access.";
export const EXAMPLE_REPOSITORY_URL = "https://github.com/agentsfleet/linkwarden";
const SOURCE_ROOT = `${EXAMPLE_REPOSITORY_URL}/blob/952ac4540657cae3a67c3ca59433899d2fda8374`;

export const EXAMPLE_JOBS = [
  {
    id: JOB_ID.incident, name: "Linkwarden watch", label: "Investigate an incident",
    request: "Watch agentsfleet/linkwarden. Investigate failed page archives and explain what needs fixing.",
    title: "Why are Linkwarden archives timing out?",
    question: "Which worker logs should Linkwarden watch use?",
    setupReply: "Read agentsfleet/linkwarden and our worker logs in Grafana. /schedule every 15 minutes.",
    setupReview: "Check every 15 minutes · UTC. Read Linkwarden and Grafana; report here. Connect Slack if you want team delivery. Review this setup before saving.",
    check: "Next scheduled check · 09:15 UTC · example time",
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
    report: "The missing timeout override explains this example’s five-minute stop. Try BROWSER_TIMEOUT=10, restart the worker, and retry the page to confirm. I’ve prepared the configuration change; nothing has been applied.",
  },
  {
    id: JOB_ID.tests, name: "Regression watch", label: "Catch test regressions",
    request: "Catch product test regressions before they reach production.",
    title: "Catch a Linkwarden archive test regression",
    question: "Which repository and test workflow should this fleet watch?",
    setupReply: "Watch agentsfleet/linkwarden and its archive URL tests. /schedule every 15 minutes.",
    setupReview: "Check product tests every 15 minutes · UTC. Read GitHub runs and code; report failures here. Review this setup before saving.",
    check: "Next scheduled check · 09:15 UTC · example time",
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
    request: "Prepare a weekly release brief from my repository.",
    title: "An agent for your weekly release brief",
    question: "What should the agent do, and how often?",
    setupReply: "Summarize changes in agentsfleet/linkwarden and flag anything needing follow-up. /schedule Mondays at 09:00 UTC.",
    setupReview: "Every Monday at 09:00 · UTC. Read the selected GitHub repository; prepare a release brief here. Review this setup before saving.",
    check: "Next Monday · 09:00 UTC · example time",
    activity: "Reading this week’s changes",
    investigation: "I found three merged pull requests. I’m reading their descriptions and test results for the release brief.",
    boundary: "This fleet reads GitHub and writes a brief here. It does not publish a release or change repository code.",
    finding: "Three changes are ready to summarize. A database migration has a rollback question that still needs an owner.",
    proposal: "Include the three changes in the brief and ask the migration owner to confirm the rollback steps before release.",
    code: ["  Weekly release brief", "+ 3 merged changes with links", "+ Follow up: confirm migration rollback steps"],
    evidence: [
      { source: "GitHub", label: "Sample weekly changes", detail: "agentsfleet/linkwarden · illustrative brief", excerpt: "Archive reliability improvements\nCollection sharing improvements\nDatabase migration" },
      { source: "GitHub", label: "Sample release follow-up", detail: "Illustrative migration notes", excerpt: "Migration: saved-link records\nRollback notes: awaiting owner confirmation" },
    ],
    report: "Your weekly brief includes three merged changes and their source links. Confirm the migration rollback steps before release. No release published; repository unchanged.",
  },
] as const satisfies readonly ExampleJob[];

export const DEFAULT_JOB = EXAMPLE_JOBS[0];
export const REQUEST_MAX_LENGTH = 1000;
export const EXAMPLE_ANCHOR = "how-it-works";
export const JOBS_ANCHOR = "operational-loop";
