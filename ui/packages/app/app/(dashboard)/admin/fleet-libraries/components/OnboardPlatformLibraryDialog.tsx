"use client";

import { useRef, useState } from "react";
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
  DialogTrigger,
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
  TooltipButton,
} from "@agentsfleet/design-system";
import { CircleHelpIcon, PlusIcon } from "lucide-react";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { SOURCE_KIND_GITHUB, type OnboardedPlatformLibraryEntry } from "@/lib/types";
import { onboardPlatformLibraryAction } from "../actions";
import {
  LIBRARY_AUTHORING_DOC_URL,
  ONBOARD_ACTION,
  ONBOARD_TOOLTIP,
  SAMPLE_LIBRARY_REPO,
  SOURCE_REF_PATTERN,
} from "../library-copy";

const OUTCOME_SUCCESS = "success";
const OUTCOME_FAILURE = "failure";

const schema = z.object({
  source_ref: z
    .string()
    .trim()
    .regex(SOURCE_REF_PATTERN, `Use owner/repo, for example ${SAMPLE_LIBRARY_REPO}`),
});

type FormValues = z.infer<typeof schema>;

export default function OnboardPlatformLibraryDialog({
  onOnboarded,
}: {
  onOnboarded: (entry: OnboardedPlatformLibraryEntry) => void;
}) {
  const [open, setOpen] = useState(false);
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);
  const [pending, setPending] = useState(false);
  // Monotonic id so a response from a submit the operator has already abandoned
  // (dialog closed, or a second submit raced past it) can never land.
  const requestIdRef = useRef(0);
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { source_ref: "" },
  });

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (next) return;
    requestIdRef.current += 1;
    setPending(false);
    setApiError(null);
    form.reset({ source_ref: "" });
  }

  async function onSubmit(values: FormValues) {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setApiError(null);
    setPending(true);
    try {
      const result = await onboardPlatformLibraryAction({
        source_kind: SOURCE_KIND_GITHUB,
        source_ref: values.source_ref,
      });
      if (requestId !== requestIdRef.current) return;
      if (!result.ok) {
        captureProductEvent(EVENTS.platform_library_onboarded, {
          source_kind: SOURCE_KIND_GITHUB,
          outcome: OUTCOME_FAILURE,
        });
        setApiError(
          presentError({
            errorCode: result.errorCode,
            message: result.error,
            action: ONBOARD_ACTION,
          }),
        );
        return;
      }
      captureProductEvent(EVENTS.platform_library_onboarded, {
        source_kind: SOURCE_KIND_GITHUB,
        outcome: OUTCOME_SUCCESS,
        entry_id: result.data.id,
      });
      onOnboarded(result.data);
      handleOpenChange(false);
    } finally {
      if (requestId === requestIdRef.current) setPending(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <TooltipButton type="button" size="sm" tooltip={ONBOARD_TOOLTIP}>
          <PlusIcon size={14} />
          Onboard fleet
        </TooltipButton>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Onboard fleet</DialogTitle>
          <DialogDescription>
            Onboard a GitHub repository whose root carries a SKILL.md. Every workspace can install
            what lands in the platform catalog.
          </DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form
            onSubmit={(e) => {
              void form.handleSubmit(onSubmit)(e);
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
              <TooltipButton type="submit" disabled={pending} tooltip={ONBOARD_TOOLTIP}>
                {pending ? <Spinner size="sm" srLabel="Onboarding fleet" /> : null}
                Onboard
              </TooltipButton>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
