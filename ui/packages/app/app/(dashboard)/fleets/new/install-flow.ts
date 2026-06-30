// Pure helpers for the state-driven install flow. No React, no network — the
// source→requirements normalisation, the connect gate, and the
// rendered state-line model live here so InstallStates stays under the length
// cap and the gating logic is unit-testable in isolation.

import type { FleetTemplateGalleryEntry } from "@/lib/types";
import { missingCredentials } from "@/lib/fleet-credentials";
import { presentErrorString } from "@/lib/errors";
import { INSTALL_STEP, type InstallStepId } from "@/lib/streaming/install-steps";

// The chosen template to install — a single gallery entry. The flow keys the
// create body off its `visibility` (platform vs tenant). github-import and paste
// sources were removed in M103; install is template-only.
export type InstallSource = FleetTemplateGalleryEntry;

// What a template needs before it can run, normalised for the install gate.
export type SourceRequirements = {
  name: string;
  credentials: string[];
  // Why each credential is needed, keyed by name (e.g. github → "review your
  // pull requests"). Platform templates carry curated copy; tenant templates
  // report an empty map and the gate falls back to its generic connect copy.
  credentialReasons: Record<string, string>;
  tools: string[];
  networkHosts: string[];
  // False when the template shipped no TRIGGER.md — create still succeeds, but
  // the skill-only state tells the operator a manual / API wake was generated.
  triggerPresent: boolean;
};

// Normalise a template's declared requirements for the install gate.
export function requirementsOf(source: InstallSource): SourceRequirements {
  return {
    name: source.name,
    credentials: source.requirements.credentials,
    credentialReasons: source.required_credentials_reasons ?? {},
    tools: source.requirements.tools,
    networkHosts: source.requirements.network_hosts,
    triggerPresent: source.requirements.trigger_present,
  };
}

// The credentials a source needs that are not present in the workspace vault.
// `present === null` means the vault could not be read — we cannot tell present
// from missing, so we gate nothing (the server's 424 stays authoritative) and
// return an empty list. Connect-to-continue resolves the rest via the custom-
// secret bridge (store e.g. GITHUB_TOKEN); the one-click connector is a later
// milestone, so this never offers an App connect.
export function unmetCredentials(
  required: readonly string[],
  present: readonly string[] | null,
): string[] {
  if (present === null) return [];
  return missingCredentials(required, present);
}

// True when the source can create immediately with no connect-gate beat.
export function readyToCreate(
  required: readonly string[],
  present: readonly string[] | null,
): boolean {
  return unmetCredentials(required, present).length === 0;
}

// Render-model for a terminal-native state line. `glyph` is the leading mono
// char (◐ running · ✓ ok · ✗ error · ○ waiting); `tone` styles it.
export type StateTone = "run" | "ok" | "err" | "wait";
export type StateLine = {
  id: string;
  tone: StateTone;
  glyph: string;
  text: string;
};

export const STATE_GLYPH: Record<StateTone, string> = {
  run: "◐",
  ok: "✓",
  err: "✗",
  wait: "○",
};

// Map an SSE-driven install step onto its rendered tone + label. `creating`/
// `provisioning` are in-flight (run); `ready` is done (ok); `error` is failed.
// `connect` is driven pre-create by the flow, not by this map.
export function stepLine(step: InstallStepId): StateLine {
  switch (step) {
    case INSTALL_STEP.CREATING:
      return { id: step, tone: "run", glyph: STATE_GLYPH.run, text: "creating fleet…" };
    case INSTALL_STEP.PROVISIONING:
      return { id: step, tone: "run", glyph: STATE_GLYPH.run, text: "provisioning — applying wake rules" };
    case INSTALL_STEP.READY:
      return { id: step, tone: "ok", glyph: STATE_GLYPH.ok, text: "ready" };
    case INSTALL_STEP.ERROR:
      return {
        id: step,
        tone: "err",
        glyph: STATE_GLYPH.err,
        text: "install failed while provisioning",
      };
    default:
      return { id: step, tone: "run", glyph: STATE_GLYPH.run, text: step };
  }
}

// Friendly label for a failed install action (import / create), threading the
// action verb through the shared presenter so every failure reads consistently.
export function flowError(
  result: { errorCode?: string; error: string },
  action: string,
): string {
  return presentErrorString({ errorCode: result.errorCode, message: result.error, action });
}
