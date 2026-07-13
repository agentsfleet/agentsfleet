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
import type { PlatformCatalogEntry, PlatformCatalogPatch } from "@/lib/types";
import { presentError, type ErrorPresentation } from "@/lib/errors";
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
} from "../library-copy";

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
  const [name, setName] = useState(entry.name);
  const [description, setDescription] = useState(entry.description);
  const [sourceRepo, setSourceRepo] = useState(entry.source_repo);
  const [sourceRef, setSourceRef] = useState(entry.source_ref);
  const [reasons, setReasons] = useState<Record<string, string>>(
    entry.required_credentials_reasons ?? {},
  );
  const [pending, setPending] = useState(false);
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);

  // The credentials the BUNDLE declares are the ones the install gate will ask
  // for, so they are the ones worth explaining. An operator cannot invent a
  // credential the fleet never requests.
  const credentials = entry.requirements.credentials;

  // Repointing the source throws the bundle away and withdraws the fleet. Say so
  // while the operator can still change their mind, not after they saved.
  const sourceChanged = sourceRepo !== entry.source_repo || sourceRef !== entry.source_ref;
  const repoMalformed = !SOURCE_REF_PATTERN.test(sourceRepo);
  const refEmpty = sourceRef.trim().length === 0;
  const nameEmpty = name.trim().length === 0;
  const blocked = pending || nameEmpty || repoMalformed || refEmpty;

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setPending(true);
    setApiError(null);
    try {
      // Send only what moved. An absent field is untouched server-side, so an
      // unchanged source never reaches the invalidation write — re-saving a
      // description must not withdraw a live fleet.
      const patch: PlatformCatalogPatch = { description, required_credentials_reasons: reasons };
      if (name !== entry.name) patch.name = name;
      if (sourceRepo !== entry.source_repo) patch.source_repo = sourceRepo;
      if (sourceRef !== entry.source_ref) patch.source_ref = sourceRef;

      const result = await patchPlatformLibraryAction(entry.id, patch);
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
