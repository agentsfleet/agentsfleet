"use client";

import type { SummaryApproval } from "./use-fleet-approval";
import { useEffect, useRef } from "react";
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
  return <dl className="space-y-xs text-label leading-label" aria-label="Requested access">
    <div><dt className="inline text-muted-foreground">Access: </dt><dd className="inline font-medium">{name}</dd></div>
    {scope ? <>
      <div className="break-words"><dt className="inline text-muted-foreground">Repositories: </dt><dd className="inline">{scope.repositories.join(", ")}</dd></div>
      <div><dt className="inline text-muted-foreground">Reach: </dt><dd className="inline">{scope.access === "write" ? "Read and write" : "Read only"}</dd></div>
    </> : null}
  </dl>;
}
