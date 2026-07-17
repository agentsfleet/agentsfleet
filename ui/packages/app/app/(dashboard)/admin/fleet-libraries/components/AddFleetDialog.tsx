"use client";

import { useEffect, useRef, useState } from "react";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { z } from "zod";
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
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
} from "@agentsfleet/design-system";
import { CircleHelpIcon } from "lucide-react";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { SOURCE_KIND_GITHUB } from "@/lib/types";
import { onboardPlatformLibraryAction } from "../actions";
import {
  ADD_ACTION,
  ADD_TOOLTIP,
  CREATE_FLEET_LIBRARY,
  FETCHING_UPDATE,
  FETCH_UPDATE,
  FETCH_UPDATE_ACTION,
  FETCH_UPDATE_DESCRIPTION,
  LIBRARY_AUTHORING_DOC_URL,
  REPLACE_ACTION,
  REPLACE_CONFIRM,
  SAMPLE_LIBRARY_REPO,
  SOURCE_REF_PATTERN,
} from "../library-copy";

const OUTCOME_SUCCESS = "success";
const OUTCOME_FAILURE = "failure";

// The server refuses a repository whose bundle name is already owned by a
// DIFFERENT repository, rather than silently swapping the content every workspace
// installs. The operator confirms the overwrite; the UI never decides it.
const ERR_ID_COLLISION = "UZ-CATALOG-004";

const schema = z.object({
  source_ref: z
    .string()
    .trim()
    .regex(SOURCE_REF_PATTERN, `Use owner/repo, for example ${SAMPLE_LIBRARY_REPO}`),
});

type FormValues = z.infer<typeof schema>;

// One dialog serves create and refetch: validation, double-submit protection,
// and error mapping stay shared while each operation keeps honest copy.
export default function AddFleetDialog({
  open,
  onOpenChange,
  prefillRepo,
  prefillRef,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** A row's repository, when the dialog was opened from that row's Fetch action. */
  prefillRepo?: string;
  /** The row's stored ref on the Fetch-update path — the pin the fetch honors. */
  prefillRef?: string;
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
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { source_ref: prefillRepo ?? "" },
  });

  useEffect(() => {
    if (open) form.reset({ source_ref: prefillRepo ?? "" });
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

  async function submit(values: FormValues, replace: boolean) {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setApiError(null);
    setPending(true);
    try {
      const result = await onboardPlatformLibraryAction({
        source_kind: SOURCE_KIND_GITHUB,
        source_ref: values.source_ref,
        // Only the refetch path pins: a fresh add fetches the default branch.
        ...(prefillRef ? { ref: prefillRef } : {}),
        ...(replace ? { replace: true } : {}),
      });
      if (requestId !== requestIdRef.current) return;
      if (!result.ok) {
        captureProductEvent(EVENTS.platform_library_onboarded, {
          source_kind: SOURCE_KIND_GITHUB,
          outcome: OUTCOME_FAILURE,
        });
        if (result.errorCode === ERR_ID_COLLISION) {
          setCollision(true);
          return;
        }
        setApiError(
          presentError({ errorCode: result.errorCode, message: result.error, action: errorAction }),
        );
        return;
      }
      captureProductEvent(EVENTS.platform_library_onboarded, {
        source_kind: SOURCE_KIND_GITHUB,
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
      <DialogContent>
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
            <FormField
              control={form.control}
              name="source_ref"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Repository</FormLabel>
                  <FormControl>
                    <Input
                      placeholder="owner/repo"
                      autoComplete="off"
                      spellCheck={false}
                      {...field}
                      readOnly={isRefetch}
                    />
                  </FormControl>
                  <FormDescription className="space-y-1">
                    <span className="block">Example: {SAMPLE_LIBRARY_REPO}</span>
                    <a
                      href={LIBRARY_AUTHORING_DOC_URL}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-flex items-center gap-1 text-pulse underline-offset-2 hover:underline focus-visible:underline"
                    >
                      <CircleHelpIcon size={13} aria-hidden="true" />
                      Learn more
                      <span className="sr-only"> about authoring fleet libraries (opens in a new tab)</span>
                    </a>
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

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
                {apiError.code ? <code className="text-xs">{apiError.code}</code> : null}
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
                {pending ? <Spinner size="sm" srLabel={isRefetch ? FETCHING_UPDATE : CREATE_FLEET_LIBRARY} /> : null}
                {dialogTitle}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
