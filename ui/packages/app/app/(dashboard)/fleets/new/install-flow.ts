// Pure helpers for the state-driven install flow. No React, no network — the
// source→requirements normalisation, the connect-to-continue gate, and the
// rendered state-line model live here so InstallStates stays under the length
// cap and the gating logic is unit-testable in isolation.

import type { BundleSnapshot, FleetTemplate } from "@/lib/types";
import { missingCredentials } from "@/lib/fleet-credentials";
import { presentErrorString } from "@/lib/errors";
import { INSTALL_STEP, type InstallStepId } from "@/lib/streaming/install-steps";

// The chosen install source. A template previews from catalog metadata; a
// GitHub source previews from its already-imported snapshot; a paste posts its
// markdown straight to create (no import step, no requirements to gate on).
export type InstallSource =
  | { kind: "template"; template: FleetTemplate }
  | { kind: "github"; snapshot: BundleSnapshot }
  | { kind: "paste"; sourceMarkdown: string; triggerMarkdown?: string };

// What a source needs before it can run, normalised across template / snapshot.
export type SourceRequirements = {
  name: string;
  credentials: string[];
  tools: string[];
  networkHosts: string[];
  // The bundle's own name, used as the create name default (snapshots know it;
  // templates resolve it server-side at import, so it stays undefined there).
  defaultName?: string;
  // False when the source shipped no TRIGGER.md — create still succeeds, but the
  // skill-only state tells the operator a manual / API wake was generated.
  triggerPresent: boolean;
};

// Normalise a source's declared requirements. Paste has none to show — its
// content is parsed server-side at create — so it reports an empty, trigger-
// present shape (the form already validated the frontmatter).
export function requirementsOf(source: InstallSource): SourceRequirements {
  if (source.kind === "template") {
    const t = source.template;
    return {
      name: t.name,
      credentials: t.required_credentials,
      tools: t.required_tools,
      networkHosts: t.network_hosts,
      triggerPresent: true,
    };
  }
  if (source.kind === "github") {
    const s = source.snapshot;
    return {
      name: s.name,
      credentials: s.requirements.credentials,
      tools: s.requirements.tools,
      networkHosts: s.requirements.network_hosts,
      defaultName: s.name,
      triggerPresent: s.requirements.trigger_present,
    };
  }
  return { name: "pasted SKILL.md", credentials: [], tools: [], networkHosts: [], triggerPresent: true };
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

// True when the source can create immediately with no connect-to-continue beat.
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
// `importing`/`connect` are driven pre-create by the flow, not by this map.
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
