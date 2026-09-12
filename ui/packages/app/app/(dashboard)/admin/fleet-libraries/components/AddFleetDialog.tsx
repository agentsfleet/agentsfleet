"use client";

import { useEffect, useRef, useState } from "react";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import {
  Alert,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Form,
  Spinner,
} from "@agentsfleet/design-system";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { GitHubSourceField, LibrarySourceTabs } from "@/components/domain/fleet-library/LibrarySourceTabs";
import {
  EMPTY_LIBRARY_SOURCE,
  librarySourcePayload,
  librarySourceSchema,
  type LibrarySourceValues,
} from "@/components/domain/fleet-library/library-source-form";
import { RETRY_CODE_TIMEOUT } from "@/lib/api/errors";
import { SOURCE_KIND_GITHUB, type PlatformCatalogEntry } from "@/lib/types";
import { onboardPlatformLibraryAction, readPlatformLibraryAction } from "../actions";
import { importLanded, repoImportState } from "../import-reconcile";
import {
  ADD_ACTION,
  ADD_TOOLTIP,
  CREATE_FLEET_LIBRARY,
  CREATE_SUBMIT,
  CREATING,
  FETCHING_UPDATE,
  FETCH_UPDATE,
  FETCH_UPDATE_ACTION,
  FETCH_UPDATE_DESCRIPTION,
  REPLACE_ACTION,
  REPLACE_CONFIRM,
} from "../library-copy";

const OUTCOME_SUCCESS = "success";
const OUTCOME_FAILURE = "failure";

// The server refuses a bundle whose name is already owned by a DIFFERENT source,
// rather than silently swapping the content every workspace installs. The
// operator confirms the overwrite; the dashboard never decides it.
const ERR_ID_COLLISION = "UZ-CATALOG-004";

// One dialog serves create and refetch: validation, double-submit protection,
// and error mapping stay shared while each operation keeps honest copy.
export default function AddFleetDialog({
  open,
  onOpenChange,
  prefillRepo,
  prefillRef,
  restoreFocus,
  entries,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /**
   * The catalog as the page last rendered it — the before half of the timeout
   * reconciliation. A prop rather than a read because the table already has it,
   * so the happy path costs no extra round-trip. Required: without the before
   * state a refetch cannot be told from a no-op, and a caller that forgot it
   * should fail to compile rather than silently reconcile worse.
   */
  entries: readonly PlatformCatalogEntry[];
  /** A row's repository, when the dialog was opened from that row's Fetch action. */
  prefillRepo?: string;
  /** The row's stored ref on the Fetch-update path — the pin the fetch honors. */
  prefillRef?: string;
  restoreFocus?: () => void;
}) {
  // Refetch is the row-driven path: a prefilled repo pins the source. An empty
  // string is not a pin, so it stays create-mode rather than rendering a broken
  // read-only dialog you can't type into.
  const isRefetch = Boolean(prefillRepo);
  const dialogTitle = isRefetch ? FETCH_UPDATE : CREATE_FLEET_LIBRARY;
  const dialogDescription = isRefetch ? FETCH_UPDATE_DESCRIPTION : ADD_TOOLTIP;
  const errorAction = isRefetch ? FETCH_UPDATE_ACTION : ADD_ACTION;
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);
  const [pending, setPending] = useState(false);
  // Set when the server reports a name collision; the operator must confirm the
  // overwrite before we retry with `replace`.
  const [collision, setCollision] = useState(false);
  // Monotonic id so a response from a submit the operator has already abandoned
  // (dialog closed, or a second submit raced past it) can never land.
  const requestIdRef = useRef(0);
  const form = useForm<LibrarySourceValues>({
    resolver: zodResolver(librarySourceSchema),
    defaultValues: { ...EMPTY_LIBRARY_SOURCE, source_ref: prefillRepo ?? "" },
  });

  useEffect(() => {
    if (open) form.reset({ ...EMPTY_LIBRARY_SOURCE, source_ref: prefillRepo ?? "" });
  }, [open, prefillRepo, form]);

  // Radix reports only closes here: the dialog is controlled and carries no
  // trigger of its own. Bumping the requestId on every close is what makes a
  // response the operator walked away from unable to land.
  function handleOpenChange(next: boolean) {
    onOpenChange(next);
    requestIdRef.current += 1;
    setPending(false);
    setApiError(null);
    setCollision(false);
  }

  /*
   * The catalog's verdict on an import we stopped waiting for.
   *
   * Only the GitHub path can be settled this way: an upload carries no
   * repository, so there is no row to match it against and the timeout stands
   * as reported. A read that itself fails also leaves the timeout standing —
   * an unanswered question is not a yes.
   */
  async function landedAfterTimeout(values: LibrarySourceValues): Promise<boolean> {
    if (values.source_kind !== SOURCE_KIND_GITHUB) return false;
    const repo = values.source_ref;
    const before = repoImportState(entries, repo);
    const reread = await readPlatformLibraryAction();
    if (!reread.ok) return false;
    return importLanded(before, repoImportState(reread.data.entries, repo));
  }

  async function submit(values: LibrarySourceValues, replace: boolean) {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setApiError(null);
    setPending(true);
    try {
      // Only the refetch path pins a ref; a fresh add fetches the default branch,
      // and an upload carries none at all.
      const result = await onboardPlatformLibraryAction(
        librarySourcePayload(values, { ...(prefillRef ? { ref: prefillRef } : {}), replace }),
      );
      if (requestId !== requestIdRef.current) return;
      if (!result.ok) {
        if (result.errorCode === ERR_ID_COLLISION) {
          captureProductEvent(EVENTS.platform_library_onboarded, {
            source_kind: values.source_kind,
            outcome: OUTCOME_FAILURE,
          });
          setCollision(true);
          return;
        }
        // A timeout is our patience running out, not the daemon stopping. Ask
        // the catalog before telling the operator this failed; the import may
        // have finished in the seconds after we stopped listening.
        if (result.errorCode === RETRY_CODE_TIMEOUT && (await landedAfterTimeout(values))) {
          if (requestId !== requestIdRef.current) return;
          captureProductEvent(EVENTS.platform_library_onboarded, {
            source_kind: values.source_kind,
            outcome: OUTCOME_SUCCESS,
          });
          handleOpenChange(false);
          return;
        }
        if (requestId !== requestIdRef.current) return;
        captureProductEvent(EVENTS.platform_library_onboarded, {
          source_kind: values.source_kind,
          outcome: OUTCOME_FAILURE,
        });
        setApiError(
          presentError({ errorCode: result.errorCode, message: result.error, action: errorAction }),
        );
        return;
      }
      captureProductEvent(EVENTS.platform_library_onboarded, {
        source_kind: values.source_kind,
        outcome: OUTCOME_SUCCESS,
        entry_id: result.data.id,
      });
      handleOpenChange(false);
    } finally {
      if (requestId === requestIdRef.current) setPending(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent onCloseAutoFocus={restoreFocus ? (event) => {
        event.preventDefault();
        restoreFocus();
      } : undefined}>
        <DialogHeader>
          <DialogTitle>{dialogTitle}</DialogTitle>
          <DialogDescription>{dialogDescription}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form
            onSubmit={(e) => {
              void form.handleSubmit((v) => submit(v, false))(e);
            }}
            className="space-y-4"
          >
            {/* Refetch re-reads a row's own source, so it offers no choice of one. */}
            {isRefetch ? (
              <GitHubSourceField form={form} readOnly />
            ) : (
              <LibrarySourceTabs
                form={form}
                disabled={pending}
                onSourceChange={() => {
                  setApiError(null);
                  setCollision(false);
                }}
              />
            )}

            {collision ? (
              <Alert variant="destructive">
                <div>{REPLACE_CONFIRM}</div>
                <Button
                  type="button"
                  variant="destructive"
                  size="sm"
                  disabled={pending}
                  onClick={() => void submit(form.getValues(), true)}
                >
                  {REPLACE_ACTION}
                </Button>
              </Alert>
            ) : null}

            {apiError ? (
              <Alert variant="destructive">
                <div>{apiError.title}</div>
                {apiError.body ? <div>{apiError.body}</div> : null}
                {apiError.code ? <code className="text-mono leading-mono">{apiError.code}</code> : null}
              </Alert>
            ) : null}

            <DialogFooter className="flex-col gap-2 sm:flex-row sm:gap-2">
              <Button
                type="button"
                variant="ghost"
                disabled={pending}
                onClick={() => handleOpenChange(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={pending}>
                {pending ? <Spinner size="sm" srLabel={isRefetch ? FETCHING_UPDATE : CREATING} /> : null}
                {isRefetch ? FETCH_UPDATE : CREATE_SUBMIT}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
