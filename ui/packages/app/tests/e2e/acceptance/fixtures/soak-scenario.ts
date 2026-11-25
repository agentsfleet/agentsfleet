/** The final acceptance journey keeps one fleet through a long chat and three pinned rules. */
const MS_PER_MINUTE = 60_000;
// Whole-journey wall clock only; every turn keeps its own turnTimeoutMs bound.
const SINGLE_TIMEOUT_MINUTES = 120;
const CONCURRENT_TIMEOUT_MINUTES = 240;
export const SOAK_FLEET_PREFIX = "soak-";
export const SOAK = {
  initialFleetMessages: 50,
  returnedFleetMessages: 20,
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

export const SOAK_KIND = {runtime: "runtime", reopen: "reopen", pin: "pin"} as const;
export const SOAK_STEPS = [
  {kind: SOAK_KIND.runtime, count: SOAK.initialFleetMessages},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.runtime, count: SOAK.returnedFleetMessages},
  {kind: SOAK_KIND.pin, rule: 0},
  {kind: SOAK_KIND.runtime, count: SOAK.laterFleetMessages},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.runtime, count: SOAK.laterFleetMessages},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.pin, rule: 1},
  {kind: SOAK_KIND.reopen},
  {kind: SOAK_KIND.runtime, count: SOAK.laterFleetMessages},
  {kind: SOAK_KIND.pin, rule: 2},
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

// Each rule widens the UTF-8 encoding: two-byte Latin, three-byte CJK and
// Hangul, then emoji with a variation selector beside four-byte astral symbols
// and right-to-left Arabic. A pinned rule is read back from the fleet's memory
// byte for byte, so the daemon's storage of each width is exercised.
export const PINNED_RULES = [
  "Keep names such as Zoë, Søren, François and Łukasz spelled exactly, and write units as “ms”, “µs” and “°C”.",
  "Treat 東京, 서울 and München as region names, never as requests to translate.",
  "Mark a risky step with ⚠️ and a safe one with ✅, and show capacity arithmetic as 𝑁 × 𝑟 ≈ 𝑇 with the team terms «оценка» and «تقدير» unchanged.",
] as const;

/** The first message: what the fleet is for, said once, as a person starts a chat. */
export function firstMessage(name: string): string {
  return `You help project ${name} with engineering decisions. ` +
    "Give concise useful answers, challenge consequential assumptions, accept factual corrections, and distinguish facts from inference. " +
    "Reply to this message with one short sentence.";
}

/** Asks the fleet to pin one standing rule, which every later run's prompt carries. */
export function pinMessage(rule: string): string {
  return "Remember this standing guidance with memory_store under the category core, exactly as written, " +
    `then confirm in one short sentence: ${rule}`;
}

export function runtimeMessage(index: number): string {
  return QUESTIONS[(index - 1) % QUESTIONS.length]!;
}
