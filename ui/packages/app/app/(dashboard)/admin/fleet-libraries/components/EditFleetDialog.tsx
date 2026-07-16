"use client";

import { useState } from "react";
import {
  Alert,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Spinner,
  Textarea,
} from "@agentsfleet/design-system";
import { CATALOG_STATUS_PUBLISHED, catalogStatus } from "@/lib/types";
import type { PlatformCatalogEntry, PlatformCatalogPatch } from "@/lib/types";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { patchPlatformLibraryAction } from "../actions";

import {
  EDIT_DESCRIPTION,
  EDIT_DESCRIPTION_LABEL,
  EDIT_NAME_LABEL,
  EDIT_REASON_LABEL,
  EDIT_REASON_MISSING,
  EDIT_SOURCE_REF_HINT,
  EDIT_SOURCE_REF_LABEL,
  EDIT_SOURCE_REPO_LABEL,
  EDIT_SOURCE_WARNING,
  EDIT_TITLE,
  PATCH_ACTION,
  SOURCE_REF_PATTERN,
  SOURCE_SEGMENT_PATTERN,
} from "../library-copy";

const FIELD_REPO = "repo";
const FIELD_REF = "ref";
const FIELD_BOTH = "both";

// The fields the operator owns, and the ones the bundle does.
//
// `description` and the per-credential copy were operator-owned from the start:
// the server drops them from the refetch upsert so an edit here survives the next
// `Fetch update` (M128 Invariant 4). M130 moves `name`, `source_repo`, and
// `source_ref` across the same line — `name` joins that refetch exclusion list,
// and a changed source discards the stored bundle, because the bundle was built
// from the OLD repository and the row must never advertise a source it is not
// serving.
//
// `id` is not here and never will be: it is the primary key, and a workspace
// install references it as `platform_library_id`.
export default function EditFleetDialog({
  entry,
  open,
  onOpenChange,
}: {
  entry: PlatformCatalogEntry;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  // The BASELINE is frozen at dialog-open. Every revalidation refreshes the
  // `entry` prop under a still-open dialog (the table re-resolves it by id), so
  // comparing against the live prop would misread another operator's mid-flight
  // change as THIS operator's edit — and send a repoint they never made. Fields
  // are compared, and sent, against what this dialog actually showed them.
  const [baseline] = useState(entry);
  const [name, setName] = useState(baseline.name);
  const [description, setDescription] = useState(baseline.description);
  const [sourceRepo, setSourceRepo] = useState(baseline.source_repo);
  const [sourceRef, setSourceRef] = useState(baseline.source_ref);
  const [reasons, setReasons] = useState<Record<string, string>>(
    baseline.required_credentials_reasons ?? {},
  );
  const [pending, setPending] = useState(false);
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);

  // The credentials the BUNDLE declares are the ones the install gate will ask
  // for, so they are the ones worth explaining. An operator cannot invent a
  // credential the fleet never requests.
  const credentials = baseline.requirements.credentials;

  // Only what MOVED is validated and sent. This is one rule doing two jobs:
  // a template/upload-sourced row carries a source that is not owner/repo shaped,
  // and it must stay copy-editable — its untouched source is never sent, so it is
  // never re-validated. And a stale dialog must not clobber another operator's
  // newer copy (or resurrect refetch-pruned reasons) by re-sending fields the
  // operator never touched.
  const nameChanged = name !== baseline.name;
  const repoChanged = sourceRepo !== baseline.source_repo;
  const refChanged = sourceRef !== baseline.source_ref;
  const descriptionChanged = description !== baseline.description;
  const reasonsChanged =
    JSON.stringify(reasons) !== JSON.stringify(baseline.required_credentials_reasons ?? {});

  // Repointing the source throws the bundle away and withdraws the fleet. Say so
  // while the operator can still change their mind, not after they saved.
  const sourceChanged = repoChanged || refChanged;
  const repoMalformed = repoChanged && !SOURCE_REF_PATTERN.test(sourceRepo);
  // Tested raw, not trimmed — the raw value is what a save sends, so a stray
  // space must block here rather than round-trip to the server's refusal.
  const refInvalid = refChanged && !SOURCE_SEGMENT_PATTERN.test(sourceRef);
  const nameInvalid = nameChanged && name.trim().length === 0;
  const blocked = pending || nameInvalid || repoMalformed || refInvalid;

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setPending(true);
    setApiError(null);
    try {
      // Send only what moved — nothing else exists on the wire. A save with
      // zero moves is a close, not a write.
      const patch: PlatformCatalogPatch = {};
      if (nameChanged) patch.name = name;
      if (descriptionChanged) patch.description = description;
      if (reasonsChanged) patch.required_credentials_reasons = reasons;
      if (repoChanged) patch.source_repo = sourceRepo;
      if (refChanged) patch.source_ref = sourceRef;
      if (Object.keys(patch).length === 0) {
        onOpenChange(false);
        return;
      }

      const result = await patchPlatformLibraryAction(baseline.id, patch, baseline.etag);

      // Repointing is the one edit here that withdraws a live fleet from every
      // workspace gallery — the operator action worth an ops signal. A refusal
      // is recorded too: a repoint nobody could complete is a signal, not an
      // absence of one. ONE event per save (1 event = 1 operator action; a
      // both-halves repoint must not double-count), and never the values
      // themselves — a repository name can carry a private org.
      if (sourceChanged) {
        captureProductEvent(EVENTS.platform_library_source_changed, {
          entry_id: baseline.id,
          field: repoChanged && refChanged ? FIELD_BOTH : repoChanged ? FIELD_REPO : FIELD_REF,
          was_published: catalogStatus(baseline) === CATALOG_STATUS_PUBLISHED,
          outcome: result.ok ? "success" : "failure",
        });
      }

      if (!result.ok) {
        setApiError(
          presentError({ errorCode: result.errorCode, message: result.error, action: PATCH_ACTION }),
        );
        return;
      }
      onOpenChange(false);
    } finally {
      setPending(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{EDIT_TITLE}</DialogTitle>
          <DialogDescription>{EDIT_DESCRIPTION}</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            void onSubmit(e);
          }}
          className="space-y-4"
        >
          <div className="space-y-2">
            <Label htmlFor="fleet-name">{EDIT_NAME_LABEL}</Label>
            <Input id="fleet-name" value={name} onChange={(e) => setName(e.target.value)} />
          </div>

          <div className="space-y-2">
            <Label htmlFor="fleet-source-repo">{EDIT_SOURCE_REPO_LABEL}</Label>
            <Input
              id="fleet-source-repo"
              value={sourceRepo}
              aria-invalid={repoMalformed}
              onChange={(e) => setSourceRepo(e.target.value)}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="fleet-source-ref">{EDIT_SOURCE_REF_LABEL}</Label>
            <Input
              id="fleet-source-ref"
              value={sourceRef}
              aria-invalid={refInvalid}
              onChange={(e) => setSourceRef(e.target.value)}
            />
            <p className="text-xs text-muted-foreground">{EDIT_SOURCE_REF_HINT}</p>
          </div>

          {sourceChanged ? (
            <Alert variant="destructive" data-testid="source-warning">
              {EDIT_SOURCE_WARNING}
            </Alert>
          ) : null}

          <div className="space-y-2">
            <Label htmlFor="fleet-description">{EDIT_DESCRIPTION_LABEL}</Label>
            <Textarea
              id="fleet-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
            />
          </div>

          {credentials.length > 0 ? (
            <div className="space-y-2">
              <Label>{EDIT_REASON_LABEL}</Label>
              {credentials.map((credential) => (
                <CredentialReason
                  key={credential}
                  credential={credential}
                  value={reasons[credential] ?? ""}
                  onChange={(next) => setReasons((prev) => ({ ...prev, [credential]: next }))}
                />
              ))}
            </div>
          ) : null}

          {apiError ? (
            <Alert variant="destructive">
              <div>{apiError.title}</div>
              {apiError.body ? <div>{apiError.body}</div> : null}
              {apiError.code ? <code className="text-xs">{apiError.code}</code> : null}
            </Alert>
          ) : null}

          <DialogFooter className="flex-col gap-2 sm:flex-row sm:gap-2">
            <Button type="button" variant="ghost" disabled={pending} onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={blocked}>
              {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// One credential's copy. An empty reason is called out rather than left blank:
// the install gate WILL ask a user for this credential, and with no copy it
// cannot say why. This marker is the only place an operator would find that out.
function CredentialReason({
  credential,
  value,
  onChange,
}: {
  credential: string;
  value: string;
  onChange: (next: string) => void;
}) {
  const unexplained = value.trim().length === 0;
  return (
    <div className="space-y-1">
      <Label htmlFor={`reason-${credential}`} className="text-xs text-muted-foreground">
        {credential}
      </Label>
      <Input
        id={`reason-${credential}`}
        value={value}
        placeholder={`why this fleet needs ${credential}`}
        onChange={(e) => onChange(e.target.value)}
      />
      {unexplained ? (
        <p data-testid={`reason-missing-${credential}`} className="text-xs text-muted-foreground">
          {EDIT_REASON_MISSING}
        </p>
      ) : null}
    </div>
  );
}
