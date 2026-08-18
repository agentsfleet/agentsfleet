"use client";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
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
  DialogTrigger,
  Form,
  Spinner,
  TooltipButton,
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { LibrarySourceTabs } from "@/components/domain/fleet-library/LibrarySourceTabs";
import {
  EMPTY_LIBRARY_SOURCE,
  librarySourcePayload,
  librarySourceSchema,
  type LibrarySourceValues,
} from "@/components/domain/fleet-library/library-source-form";
import { onboardLibraryEntryAction } from "../actions";
import { CREATE_FLEET_LIBRARY_TOOLTIP } from "./library-docs";

const ONBOARD_ACTION = "create the fleet library";
const DIALOG_TITLE = "Create fleet library";
const DIALOG_DESCRIPTION =
  "Create from a GitHub repository that contains a fleet library entry, or from a bundle directory on this machine.";
const SUBMIT_LABEL = "Create";
const SUBMITTING_LABEL = "Creating fleet library";
const OUTCOME_SUCCESS = "success";

type Props = {
  workspaceId: string;
  triggerLabel?: string;
  /** Open the dialog on first render (e.g. the ?create=1 deep link). */
  defaultOpen?: boolean;
};

export default function AddLibraryDialog({
  workspaceId,
  triggerLabel = DIALOG_TITLE,
  defaultOpen = false,
}: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(defaultOpen);
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);
  const [pending, setPending] = useState(false);
  // Monotonic id so a response from a submit the operator has already abandoned
  // (dialog closed, or a second submit raced past it) can never land.
  const requestIdRef = useRef(0);
  const form = useForm<LibrarySourceValues>({
    resolver: zodResolver(librarySourceSchema),
    defaultValues: EMPTY_LIBRARY_SOURCE,
  });

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (next) return;
    requestIdRef.current += 1;
    setPending(false);
    setApiError(null);
    form.reset(EMPTY_LIBRARY_SOURCE);
  }

  async function onSubmit(values: LibrarySourceValues) {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setApiError(null);
    setPending(true);
    try {
      const result = await onboardLibraryEntryAction(workspaceId, librarySourcePayload(values));
      if (requestId !== requestIdRef.current) return;
      if (!result.ok) {
        setApiError(presentError({
          errorCode: result.errorCode,
          message: result.error,
          action: ONBOARD_ACTION,
        }));
        return;
      }
      captureProductEvent(EVENTS.fleet_library_onboarded, {
        workspace_id: workspaceId,
        visibility: result.data.visibility,
        source_kind: values.source_kind,
        outcome: OUTCOME_SUCCESS,
      });
      handleOpenChange(false);
      router.refresh();
    } finally {
      if (requestId === requestIdRef.current) setPending(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <TooltipButton type="button" size="sm" tooltip={CREATE_FLEET_LIBRARY_TOOLTIP}>
          <PlusIcon size={14} />
          {triggerLabel}
        </TooltipButton>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{DIALOG_TITLE}</DialogTitle>
          <DialogDescription>{DIALOG_DESCRIPTION}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }} className="space-y-4">
            <LibrarySourceTabs form={form} disabled={pending} onSourceChange={() => setApiError(null)} />
            {apiError ? (
              <Alert variant="destructive">
                <div>{apiError.title}</div>
                {apiError.body ? <div>{apiError.body}</div> : null}
                {apiError.code ? <code className="text-xs">{apiError.code}</code> : null}
              </Alert>
            ) : null}
            <DialogFooter className="flex-col gap-2 sm:flex-row sm:gap-2">
              <Button type="button" variant="ghost" disabled={pending} onClick={() => handleOpenChange(false)}>
                Cancel
              </Button>
              <TooltipButton type="submit" disabled={pending} tooltip={CREATE_FLEET_LIBRARY_TOOLTIP}>
                {pending ? <Spinner size="sm" srLabel={SUBMITTING_LABEL} /> : null}
                {SUBMIT_LABEL}
              </TooltipButton>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
