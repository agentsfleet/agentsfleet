"use client";

import type { SummaryApproval } from "./use-fleet-approval";
import { useEffect, useRef } from "react";
import { DescriptionDetails, DescriptionList, DescriptionTerm } from "@agentsfleet/design-system";
import { GITHUB_GRANT, GRANT_STATUS, githubScope } from "@/lib/api/fleet-detail";
import { useFleetLists } from "../fleet/FleetListsProvider";

/** What an access card asks for: the name, and for GitHub the repositories and how far, read from the pending grant. */
export function AccessRequest({gate}: {gate: SummaryApproval}) {
  const lists = useFleetLists();
  const name = typeof gate.evidence?.service === "string" ? gate.evidence.service : null;
  const grants = lists?.grants;
  const grant = grants?.kind === "ready" ? grants.items.find(item => item.name === name && item.status === GRANT_STATUS.pending) : undefined;
  const missing = grants?.kind === "ready" && grant === undefined;
  const reload = lists?.reload;
  const asked = useRef<string | null>(null);
  // A card raised after the lists were read names a grant they do not hold yet: read them once more for it.
  useEffect(() => {
    if (!missing || !reload || asked.current === gate.gate_id) return;
    asked.current = gate.gate_id;
    reload();
  }, [gate.gate_id, missing, reload]);
  if (!name) return null;
  const scope = name === GITHUB_GRANT ? githubScope(grant?.requested_scope) : null;
  return <DescriptionList layout="stacked" className="space-y-md text-label leading-label" aria-label="Requested access">
    <div><DescriptionTerm className="text-eyebrow leading-eyebrow">Access</DescriptionTerm><DescriptionDetails className="wrap-anywhere font-medium">{name}</DescriptionDetails></div>
    {scope ? <>
      <div><DescriptionTerm className="text-eyebrow leading-eyebrow">Repositories</DescriptionTerm>
        <DescriptionDetails className="wrap-anywhere">{scope.repositories.join(", ")}</DescriptionDetails></div>
      <div><DescriptionTerm className="text-eyebrow leading-eyebrow">Permissions</DescriptionTerm>
        <DescriptionDetails>{scope.access === "write" ? "Read and write" : "Read only"}</DescriptionDetails></div>
    </> : null}
  </DescriptionList>;
}
