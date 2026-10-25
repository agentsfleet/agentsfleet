/** The final acceptance journey keeps one fleet through four saved setups. */
const MS_PER_MINUTE = 60_000;
const SINGLE_TIMEOUT_MINUTES = 30;
const CONCURRENT_TIMEOUT_MINUTES = 60;
export const CONTINUITY_FLEET_PREFIX = "continuity-";

export const CONTINUITY = {
  authoredMessages: 50,
  initialFleetMessages: 50,
  returnedFleetMessages: 20,
  setupMessages: 10,
  laterFleetMessages: 10,
  concurrentFleets: 10,
  turnTimeoutMs: 180_000,
  singleTimeoutMs: SINGLE_TIMEOUT_MINUTES * MS_PER_MINUTE,
  concurrentTimeoutMs: CONCURRENT_TIMEOUT_MINUTES * MS_PER_MINUTE,
  pageLimit: 25,
  maxHistoryPages: 12,
} as const;

export const CONTINUITY_KIND = {runtime: "runtime", reopen: "reopen", edit: "edit"} as const;
export const CONTINUITY_STEPS = [
  {kind: CONTINUITY_KIND.runtime, count: CONTINUITY.initialFleetMessages},
  {kind: CONTINUITY_KIND.reopen},
  {kind: CONTINUITY_KIND.runtime, count: CONTINUITY.returnedFleetMessages},
  {kind: CONTINUITY_KIND.edit, revision: 2},
  {kind: CONTINUITY_KIND.runtime, count: CONTINUITY.laterFleetMessages},
  {kind: CONTINUITY_KIND.reopen},
  {kind: CONTINUITY_KIND.runtime, count: CONTINUITY.laterFleetMessages},
  {kind: CONTINUITY_KIND.reopen},
  {kind: CONTINUITY_KIND.edit, revision: 3},
  {kind: CONTINUITY_KIND.reopen},
  {kind: CONTINUITY_KIND.runtime, count: CONTINUITY.laterFleetMessages},
  {kind: CONTINUITY_KIND.edit, revision: 4},
  {kind: CONTINUITY_KIND.runtime, count: 1},
] as const;

const QUESTIONS = [
  "We serve twenty requests a minute. Should we add a cache now?",
  "Our release is late. Should we remove every automated test to catch up?",
  "A customer reports an intermittent error. What evidence should we collect first?",
  "Two implementations are equally fast. How should we choose between them?",
  "When would you challenge my assumption rather than answer directly?",
  "I corrected a factual claim you made. How should you respond?",
  "We have no measurements yet. Can you honestly recommend a database migration?",
  "How should we decide whether to roll back a release?",
  "What is the difference between a concrete caveat and vague uncertainty?",
  "What would make you change a recommendation?",
] as const;

const RULES = [
  "Use one concrete counterexample when challenging a consequential assumption.",
  "Give ordinary factual questions direct answers.",
  "Ask one clarifying question when missing context would materially change an answer.",
  "Accept factual corrections and acknowledge uncertainty without inventing evidence.",
  "Distinguish observed evidence from inference when recommending an action.",
  "State the recommendation first and its strongest caveat immediately afterward.",
  "Explain meaningful tradeoffs when reasonable options differ.",
  "Name the evidence that would change a recommendation.",
  "Suggest a small reversible next step when evidence is insufficient.",
] as const;

const EDIT_INTERVAL = 5;
const REVISION_RULES = [
  "Tailor recommendations to a solo developer maintaining a small service.",
  "When discussing incidents, separate immediate recovery from longer term prevention.",
  "When discussing a migration, identify one reversible experiment before a full rollout.",
] as const;

export function initialSetupMessage(name: string): string {
  return `Create a fleet named ${name} that helps with engineering decisions. ` +
    "Give concise useful answers, challenge consequential assumptions, accept factual corrections, and distinguish facts from inference. " +
    "Use no tools, credentials, triggers, or network access. " +
    "Do not save yet.";
}

export function setupMessage(index: number, revision: number): {text: string; edits: boolean} {
  if (revision > 1 && index === 0) return {
    text: `Add this instruction: ${REVISION_RULES[(revision - 2) % REVISION_RULES.length]} ` +
      "Keep the fleet name, capabilities, and other behavior unchanged. Do not save yet.",
    edits: true,
  };
  if (revision === 1 && index > 0 && index % EDIT_INTERVAL === 0) return {
    text: `Add this instruction while preserving the fleet name and existing rules: ${RULES[(index / EDIT_INTERVAL - 1) % RULES.length]}`,
    edits: true,
  };
  return {text: `${QUESTIONS[index % QUESTIONS.length]} Explain in at most thirty words without changing the setup.`, edits: false};
}

export function runtimeMessage(index: number): string {
  return QUESTIONS[(index - 1) % QUESTIONS.length]!;
}
