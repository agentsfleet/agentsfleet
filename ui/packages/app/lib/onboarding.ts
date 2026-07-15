// Onboarding step derivation — the ONE pure function the checklist page, the
// sidebar widget, and the landing logic all read, so they agree by
// construction (Invariant 5, "step derivation single-sourced"). No component
// re-derives any of this; there is exactly one source of the step states.

// The live signals every step derives from. All six are read from endpoints
// that already exist — there is no onboarding-detection backend. `modelConfigured`
// is the provider view reporting a non-empty model; `cliTicked` is the one
// signal the server cannot detect (a CLI install), so it is a manual, persisted
// tick. See `deriveSteps` for how each maps to a step.
export type OnboardingInputs = {
  modelConfigured: boolean;
  fleetTotal: number;
  secretCount: number;
  hasProcessedEvent: boolean;
  hasSteerEvent: boolean;
  cliTicked: boolean;
};

export type OnboardingStepId =
  | "model_configured"
  | "install_fleet"
  | "connect_credential"
  | "watch_wake"
  | "steer"
  | "install_cli";

export type OnboardingStep = {
  id: OnboardingStepId;
  label: string;
  hint: string;
  done: boolean;
  required: boolean;
  // A workspace-relative path the row links to, or null when the step has no
  // destination (it completes by activity elsewhere, not by navigation).
  href: string | null;
  // The single "next" step the rail rings — the first incomplete REQUIRED step
  // in fixed order. Exactly one step (or none, when all required are done)
  // carries this. Optional steps are never the ringed "next".
  isNext: boolean;
};

// The step catalogue, in fixed render order. Labels and hints live here so the
// page and widget render identical copy. `done` is filled by `deriveSteps`.
const STEP_TEMPLATES: ReadonlyArray<{
  id: OnboardingStepId;
  label: string;
  hint: string;
  required: boolean;
  href: string | null;
  doneOf: (i: OnboardingInputs) => boolean;
}> = [
  {
    id: "model_configured",
    label: "Model configured",
    // Ticked by default: a fresh tenant rides the platform default, so the
    // operator's first sight of the checklist already shows progress. It unticks
    // only when no model exists anywhere — then the row links to Models.
    hint: "Running on the platform default. Bring your own key any time.",
    required: true,
    href: "settings/models",
    doneOf: (i) => i.modelConfigured,
  },
  {
    id: "install_fleet",
    label: "Install a fleet",
    hint: "Start from the prebuilt library. GitHub PR reviewer is a good first one.",
    required: true,
    href: "fleets/new",
    doneOf: (i) => i.fleetTotal >= 1,
  },
  {
    id: "connect_credential",
    label: "Connect its credential",
    hint: "It'll ask for what it needs, and tell you why, at the install gate.",
    required: true,
    href: "settings/secrets",
    doneOf: (i) => i.secretCount >= 1,
  },
  {
    id: "watch_wake",
    label: "Watch it wake",
    hint: "Your fleets live on the wall. That's where you steer them.",
    required: true,
    href: null,
    doneOf: (i) => i.hasProcessedEvent,
  },
  {
    id: "steer",
    label: "Steer it",
    hint: "Send it a message from the console and watch it act.",
    required: true,
    href: null,
    doneOf: (i) => i.hasSteerEvent,
  },
  {
    id: "install_cli",
    label: "Install the CLI",
    // Not server-detectable, so this is a manual tick the user sets themselves.
    hint: "npm install -g @agentsfleet/cli@next — tick it yourself; we can't detect it.",
    required: false,
    href: null,
    doneOf: (i) => i.cliTicked,
  },
];

// Derive the ordered step list from live inputs. The first incomplete REQUIRED
// step (in fixed order) is flagged `isNext` — that's the one the rail rings.
export function deriveSteps(inputs: OnboardingInputs): OnboardingStep[] {
  let nextAssigned = false;
  return STEP_TEMPLATES.map((t) => {
    const done = t.doneOf(inputs);
    const isNext = !done && t.required && !nextAssigned;
    if (isNext) nextAssigned = true;
    return {
      id: t.id,
      label: t.label,
      hint: t.hint,
      required: t.required,
      href: t.href,
      done,
      isNext,
    };
  });
}

// Onboarding is complete when every REQUIRED step is done. The optional CLI
// step never blocks completion (Dimension 3.3) — a user who finishes the five
// required steps is done whether or not they ever tick the CLI.
export function isOnboardingComplete(inputs: OnboardingInputs): boolean {
  return STEP_TEMPLATES.every((t) => !t.required || t.doneOf(inputs));
}

// How many required steps are done — the widget/page progress count ("2/5").
export function completedRequiredCount(inputs: OnboardingInputs): number {
  return STEP_TEMPLATES.filter((t) => t.required && t.doneOf(inputs)).length;
}

export const REQUIRED_STEP_COUNT = STEP_TEMPLATES.filter((t) => t.required).length;
