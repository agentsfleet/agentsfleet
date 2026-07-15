import { describe, expect, it } from "vitest";
import {
  completedRequiredCount,
  deriveSteps,
  isOnboardingComplete,
  REQUIRED_STEP_COUNT,
  type OnboardingInputs,
} from "./onboarding";

const ZERO: OnboardingInputs = {
  modelConfigured: false,
  fleetTotal: 0,
  secretCount: 0,
  hasProcessedEvent: false,
  hasSteerEvent: false,
  cliTicked: false,
};

const ALL_REQUIRED: OnboardingInputs = {
  modelConfigured: true,
  fleetTotal: 1,
  secretCount: 1,
  hasProcessedEvent: true,
  hasSteerEvent: true,
  cliTicked: false,
};

function doneIds(inputs: OnboardingInputs): string[] {
  return deriveSteps(inputs).filter((s) => s.done).map((s) => s.id);
}

describe("deriveSteps — required steps from live state (3.1)", () => {
  it("all five required steps are false when every signal is false", () => {
    const steps = deriveSteps(ZERO);
    const required = steps.filter((s) => s.required);
    expect(required).toHaveLength(REQUIRED_STEP_COUNT);
    expect(required.every((s) => !s.done)).toBe(true);
  });

  it("all five required steps are true when every signal is set", () => {
    const required = deriveSteps(ALL_REQUIRED).filter((s) => s.required);
    expect(required.every((s) => s.done)).toBe(true);
  });

  it("each required signal flips exactly its own step", () => {
    expect(doneIds({ ...ZERO, modelConfigured: true })).toEqual(["model_configured"]);
    expect(doneIds({ ...ZERO, fleetTotal: 1 })).toEqual(["install_fleet"]);
    expect(doneIds({ ...ZERO, secretCount: 1 })).toEqual(["connect_credential"]);
    expect(doneIds({ ...ZERO, hasProcessedEvent: true })).toEqual(["watch_wake"]);
    expect(doneIds({ ...ZERO, hasSteerEvent: true })).toEqual(["steer"]);
  });
});

describe("model step ticked by default (3.5)", () => {
  it("model_configured is done and links to Models when a model is configured", () => {
    const model = deriveSteps({ ...ZERO, modelConfigured: true }).find(
      (s) => s.id === "model_configured",
    )!;
    expect(model.done).toBe(true);
    expect(model.href).toBe("settings/models");
  });

  it("model_configured is the ringed next step and links to Models when no model exists", () => {
    const model = deriveSteps(ZERO).find((s) => s.id === "model_configured")!;
    expect(model.done).toBe(false);
    expect(model.isNext).toBe(true);
    expect(model.href).toBe("settings/models");
  });
});

describe("the optional CLI step (3.2)", () => {
  it("derives from the cli tick, independent of every required step", () => {
    const cli = deriveSteps({ ...ZERO, cliTicked: true }).find((s) => s.id === "install_cli")!;
    expect(cli.done).toBe(true);
    expect(cli.required).toBe(false);
    // Ticking the CLI advances nothing required.
    expect(completedRequiredCount({ ...ZERO, cliTicked: true })).toBe(0);
  });

  it("is never the ringed next step", () => {
    // Every required step done, CLI still pending: nothing should be `isNext`.
    const steps = deriveSteps(ALL_REQUIRED);
    expect(steps.every((s) => !s.isNext)).toBe(true);
  });
});

describe("completion requires only the required steps (3.3)", () => {
  it("five required done + zero optional → complete", () => {
    expect(isOnboardingComplete(ALL_REQUIRED)).toBe(true);
  });

  it("four required + optional ticked → not complete", () => {
    const fourPlusOptional: OnboardingInputs = {
      ...ALL_REQUIRED,
      hasSteerEvent: false,
      cliTicked: true,
    };
    expect(isOnboardingComplete(fourPlusOptional)).toBe(false);
    expect(completedRequiredCount(fourPlusOptional)).toBe(4);
  });
});

describe("the ringed next step (rail focus)", () => {
  it("is the first incomplete required step in fixed order", () => {
    // Model done, fleet not: the ring moves to install_fleet.
    const steps = deriveSteps({ ...ZERO, modelConfigured: true });
    const next = steps.filter((s) => s.isNext);
    expect(next).toHaveLength(1);
    expect(next[0]?.id).toBe("install_fleet");
  });

  it("is absent once all required steps are done", () => {
    expect(deriveSteps(ALL_REQUIRED).some((s) => s.isNext)).toBe(false);
  });
});
