/** The final acceptance journey keeps one fleet through four saved setups. */
const MS_PER_MINUTE = 60_000;
// Whole-journey wall clock only; every turn keeps its own turnTimeoutMs bound.
const SINGLE_TIMEOUT_MINUTES = 120;
const CONCURRENT_TIMEOUT_MINUTES = 240;
export const SOAK_FLEET_PREFIX = "soak-";
export const SOAK = {
  authoredMessages: 50,
  initialFleetMessages: 50,
  returnedFleetMessages: 20,
  setupMessages: 10,
  laterFleetMessages: 10,
  // The dev API is one shared-CPU machine, so the everyday run is two fleets;
  // the marathon's ten is for a production-sized target.
  miniFleets: 2,
  marathonFleets: 10,
  turnTimeoutMs: 180_000,
  singleTimeoutMs: SINGLE_TIMEOUT_MINUTES * MS_PER_MINUTE,
  concurrentTimeoutMs: CONCURRENT_TIMEOUT_MINUTES * MS_PER_MINUTE,
  pageLimit: 25,
  maxHistoryPages: 12,
} as const;

export const SOAK_KIND = {runtime: "runtime", reopen: "reopen", edit: "edit"} as const;
export const SOAK_STEPS = [
  {kind: SOAK_KIND.runtime, count: SOAK.initialFleetMessages},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.runtime, count: SOAK.returnedFleetMessages},
  {kind: SOAK_KIND.edit, revision: 2},
  {kind: SOAK_KIND.runtime, count: SOAK.laterFleetMessages},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.runtime, count: SOAK.laterFleetMessages},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.edit, revision: 3},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.runtime, count: SOAK.laterFleetMessages},
  {kind: SOAK_KIND.edit, revision: 4},
  {kind: SOAK_KIND.runtime, count: 1},
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
  "Our p99 latency is 850 ms in Zürich but 120 ms in 東京. Where should we look first?",
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

export const INSTRUCTION_CHECK = {append: "append", replace: "replace"} as const;
/** What the saved instructions must look like after a Unicode edit, judged byte for byte. */
export type InstructionCheck =
  | {kind: typeof INSTRUCTION_CHECK.append; text: string}
  | {kind: typeof INSTRUCTION_CHECK.replace; from: string; to: string};
export type SetupRequest = {text: string; edits: boolean; check?: InstructionCheck};

// Each rule widens the UTF-8 encoding: two-byte Latin, three-byte CJK and Hangul,
// emoji with a variation selector, then four-byte astral symbols beside
// right-to-left Arabic. The replace lands after earlier multibyte text, so the
// daemon's byte positions are exercised, not just its storage.
const UNICODE_RULE = {
  latin: "Keep names such as Zoë, Søren, François and Łukasz spelled exactly, and write units as “ms”, “µs” and “°C”.",
  cjk: "Treat 東京, 서울 and München as region names, never as requests to translate.",
  emoji: "Mark a risky step with ⚠️ and a safe one with ✅.",
  astral: "Show capacity arithmetic as 𝑁 × 𝑟 ≈ 𝑇, label it 📈, and keep the team terms «оценка» and «تقدير» unchanged.",
} as const;
const UNICODE_REPLACE = {from: "東京", to: "大阪"} as const;
const UNICODE_SETUP_INDEX = 5;
/** Every Unicode rule as the fourth saved setup must still hold it, after the replace. */
export const UNICODE_SAVED = [UNICODE_RULE.latin,
  UNICODE_RULE.cjk.replace(UNICODE_REPLACE.from, UNICODE_REPLACE.to), UNICODE_RULE.emoji, UNICODE_RULE.astral] as const;
/** Revision-one edit ordinals (index / EDIT_INTERVAL) that carry a Unicode rule. */
const AUTHORED_UNICODE = new Map<number, string>([[3, UNICODE_RULE.latin], [6, UNICODE_RULE.cjk]]);
const KEEP_REST = "Keep the fleet name, capabilities, and other behavior unchanged. Do not save yet.";

function unicodeAppend(rule: string, preface: string, suffix = ""): SetupRequest {
  return {text: `${preface} ${rule}${suffix ? ` ${suffix}` : ""}`, edits: true,
    check: {kind: INSTRUCTION_CHECK.append, text: rule}};
}

function revisionUnicode(revision: number): SetupRequest {
  const preface = "Add this instruction exactly as written:";
  if (revision === 2) return unicodeAppend(UNICODE_RULE.emoji, preface, KEEP_REST);
  if (revision === 3) return {
    text: `In the instructions, replace “${UNICODE_REPLACE.from}” with “${UNICODE_REPLACE.to}” and change nothing else. ${KEEP_REST}`,
    edits: true, check: {kind: INSTRUCTION_CHECK.replace, ...UNICODE_REPLACE},
  };
  return unicodeAppend(UNICODE_RULE.astral, preface, KEEP_REST);
}

export function setupMessage(index: number, revision: number): SetupRequest {
  if (revision > 1 && index === 0) return {
    text: `Add this instruction: ${REVISION_RULES[(revision - 2) % REVISION_RULES.length]} ${KEEP_REST}`,
    edits: true,
  };
  if (revision > 1 && index === UNICODE_SETUP_INDEX) return revisionUnicode(revision);
  if (revision === 1 && index > 0 && index % EDIT_INTERVAL === 0) {
    const ordinal = index / EDIT_INTERVAL;
    const unicode = AUTHORED_UNICODE.get(ordinal);
    if (unicode) return unicodeAppend(unicode, "Add this instruction exactly as written, while preserving the fleet name and existing rules:");
    return {
      text: `Add this instruction while preserving the fleet name and existing rules: ${RULES[(ordinal - 1) % RULES.length]}`,
      edits: true,
    };
  }
  return {text: `${QUESTIONS[index % QUESTIONS.length]} Explain in at most thirty words without changing the setup.`, edits: false};
}

export function runtimeMessage(index: number): string {
  return QUESTIONS[(index - 1) % QUESTIONS.length]!;
}
