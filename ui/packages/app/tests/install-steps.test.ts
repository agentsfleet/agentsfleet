import { describe, expect, it } from "vitest";
import { FRAME_KIND, type LiveFrame } from "@/lib/api/events";
import {
  advanceInstallStep,
  INSTALL_STEP,
  installStepFromKind,
  isInstallComplete,
  isInstallFrame,
  rankOf,
} from "@/lib/streaming/install-steps";

// Pure model behind the SSE-driven install steps. These pin the kind→step
// contract and the forward-only advance the registry relies on.

describe("installStepFromKind — the cross-tier frame→step map", () => {
  it("maps each install:* kind to its step, and non-install kinds to null", () => {
    expect(installStepFromKind(FRAME_KIND.INSTALL_CREATING)).toBe(INSTALL_STEP.CREATING);
    expect(installStepFromKind(FRAME_KIND.INSTALL_PROVISIONING)).toBe(INSTALL_STEP.PROVISIONING);
    expect(installStepFromKind(FRAME_KIND.INSTALL_READY)).toBe(INSTALL_STEP.READY);
    expect(installStepFromKind(FRAME_KIND.INSTALL_ERROR)).toBe(INSTALL_STEP.ERROR);
    expect(installStepFromKind(FRAME_KIND.CHUNK)).toBeNull();
    expect(installStepFromKind("totally_unknown")).toBeNull();
  });

  it("the kind values are the agreed contract strings (mirror the Zig publisher)", () => {
    expect(FRAME_KIND.INSTALL_CREATING).toBe("install:creating");
    expect(FRAME_KIND.INSTALL_PROVISIONING).toBe("install:provisioning");
    expect(FRAME_KIND.INSTALL_READY).toBe("install:ready");
    expect(FRAME_KIND.INSTALL_ERROR).toBe("install:error");
  });
});

describe("isInstallFrame", () => {
  it("is true for install frames and false for chat frames", () => {
    expect(isInstallFrame({ kind: FRAME_KIND.INSTALL_CREATING } as LiveFrame)).toBe(true);
    expect(isInstallFrame({ kind: FRAME_KIND.INSTALL_READY } as LiveFrame)).toBe(true);
    expect(
      isInstallFrame({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "e", actor: "fleet" }),
    ).toBe(false);
    expect(isInstallFrame({ kind: FRAME_KIND.CHUNK, event_id: "e", text: "hi" })).toBe(false);
  });
});

describe("rankOf — the monotonic SSE ladder", () => {
  it("ranks creating < provisioning < ready; off-ladder steps are -1", () => {
    expect(rankOf(INSTALL_STEP.CREATING)).toBeLessThan(rankOf(INSTALL_STEP.PROVISIONING));
    expect(rankOf(INSTALL_STEP.PROVISIONING)).toBeLessThan(rankOf(INSTALL_STEP.READY));
    expect(rankOf(INSTALL_STEP.IMPORTING)).toBe(-1);
    expect(rankOf(INSTALL_STEP.ERROR)).toBe(-1);
  });
});

describe("advanceInstallStep — forward-only with terminal error", () => {
  it("seeds from null, advances forward, and ignores a backward/duplicate frame", () => {
    expect(advanceInstallStep(null, INSTALL_STEP.CREATING)).toBe(INSTALL_STEP.CREATING);
    expect(advanceInstallStep(INSTALL_STEP.CREATING, INSTALL_STEP.PROVISIONING)).toBe(
      INSTALL_STEP.PROVISIONING,
    );
    // A late duplicate `creating` after `provisioning` must not rewind the spinner.
    expect(advanceInstallStep(INSTALL_STEP.PROVISIONING, INSTALL_STEP.CREATING)).toBe(
      INSTALL_STEP.PROVISIONING,
    );
    // Same-step frame holds.
    expect(advanceInstallStep(INSTALL_STEP.READY, INSTALL_STEP.READY)).toBe(INSTALL_STEP.READY);
  });

  it("error always wins and is sticky thereafter", () => {
    expect(advanceInstallStep(INSTALL_STEP.CREATING, INSTALL_STEP.ERROR)).toBe(INSTALL_STEP.ERROR);
    // Once errored, a later non-error frame cannot un-error it.
    expect(advanceInstallStep(INSTALL_STEP.ERROR, INSTALL_STEP.PROVISIONING)).toBe(
      INSTALL_STEP.ERROR,
    );
    expect(advanceInstallStep(INSTALL_STEP.ERROR, INSTALL_STEP.READY)).toBe(INSTALL_STEP.ERROR);
  });
});

describe("isInstallComplete — the installing→active flip signal", () => {
  it("is true only at the ready step", () => {
    expect(isInstallComplete(INSTALL_STEP.READY)).toBe(true);
    expect(isInstallComplete(INSTALL_STEP.PROVISIONING)).toBe(false);
    expect(isInstallComplete(INSTALL_STEP.ERROR)).toBe(false);
    expect(isInstallComplete(null)).toBe(false);
  });
});
