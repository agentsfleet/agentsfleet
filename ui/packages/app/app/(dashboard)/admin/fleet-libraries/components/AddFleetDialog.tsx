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
  ADD_FLEET,
  ADD_TOOLTIP,
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

// One dialog serves both "Add fleet" and a row's "Fetch update": same validation,
// same double-submit guard, same error mapping — differing only in a prefilled
// repository. A second form would be a second place for the validation to drift.
export default function AddFleetDialog({
  open,
  onOpenChange,
  prefillRepo,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** A row's repository, when the dialog was opened from that row's Fetch action. */
  prefillRepo?: string;
}) {
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
          presentError({ errorCode: result.errorCode, message: result.error, action: ADD_ACTION }),
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
          <DialogTitle>{ADD_FLEET}</DialogTitle>
          <DialogDescription>{ADD_TOOLTIP}</DialogDescription>
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
                    <Input placeholder="owner/repo" autoComplete="off" spellCheck={false} {...field} />
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
                {pending ? <Spinner size="sm" srLabel="Adding fleet" /> : null}
                {ADD_FLEET}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
