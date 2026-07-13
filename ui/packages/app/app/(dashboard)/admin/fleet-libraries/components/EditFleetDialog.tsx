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
import type { PlatformCatalogEntry } from "@/lib/types";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { patchPlatformLibraryAction } from "../actions";
import {
  EDIT_DESCRIPTION,
  EDIT_DESCRIPTION_LABEL,
  EDIT_REASON_LABEL,
  EDIT_TITLE,
  PATCH_ACTION,
} from "../library-copy";

// The two fields no bundle can supply, and the only two an operator owns.
//
// `description` is seeded from the bundle when a fleet is first added, then
// belongs to the operator: the server drops it from the refetch upsert precisely
// so an edit here survives the next `Fetch update` (M128 Invariant 4). The
// per-credential copy is what a user reads at the install gate when the fleet asks
// for their token — it is the platform's voice, which is why a repository author
// cannot write it.
export default function EditFleetDialog({
  entry,
  open,
  onOpenChange,
}: {
  entry: PlatformCatalogEntry;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [description, setDescription] = useState(entry.description);
  const [reasons, setReasons] = useState<Record<string, string>>(
    entry.required_credentials_reasons ?? {},
  );
  const [pending, setPending] = useState(false);
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);

  // The credentials the BUNDLE declares are the ones the install gate will ask
  // for, so they are the ones worth explaining. An operator cannot invent a
  // credential here that the fleet never requests.
  const credentials = entry.requirements.credentials;

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setPending(true);
    setApiError(null);
    try {
      const result = await patchPlatformLibraryAction(entry.id, {
        description,
        required_credentials_reasons: reasons,
      });
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
                <div key={credential} className="space-y-1">
                  <Label htmlFor={`reason-${credential}`} className="text-xs text-muted-foreground">
                    {credential}
                  </Label>
                  <Input
                    id={`reason-${credential}`}
                    value={reasons[credential] ?? ""}
                    placeholder={`why this fleet needs ${credential}`}
                    onChange={(e) =>
                      setReasons((prev) => ({ ...prev, [credential]: e.target.value }))
                    }
                  />
                </div>
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
            <Button type="submit" disabled={pending}>
              {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
