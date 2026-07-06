"use client";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
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
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { SOURCE_KIND_GITHUB } from "@/lib/types";
import { onboardLibraryEntryAction } from "../actions";
import { CREATE_LIBRARY_DOC_URL } from "./library-docs";

const SOURCE_REF_PATTERN = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
const ONBOARD_ACTION = "create the fleet library";

const schema = z.object({
  source_ref: z
    .string()
    .trim()
    .regex(SOURCE_REF_PATTERN, "Use owner/repo, for example agentsfleet/github-pr-reviewer"),
});

type FormValues = z.infer<typeof schema>;

type Props = {
  workspaceId: string;
  triggerLabel?: string;
  /** Open the dialog on first render (e.g. the ?create=1 deep link). */
  defaultOpen?: boolean;
};

export default function AddLibraryDialog({
  workspaceId,
  triggerLabel = "Create fleet library",
  defaultOpen = false,
}: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(defaultOpen);
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);
  const [pending, setPending] = useState(false);
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
    const sourceRef = values.source_ref;
    try {
      const result = await onboardLibraryEntryAction(workspaceId, {
        source_kind: SOURCE_KIND_GITHUB,
        source_ref: sourceRef,
      });
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
        source_kind: SOURCE_KIND_GITHUB,
        outcome: "success",
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
        <Button type="button" size="sm">
          <PlusIcon size={14} />
          {triggerLabel}
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create fleet library</DialogTitle>
          <DialogDescription>
            Add a GitHub repository that contains a fleet library entry.
          </DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }} className="space-y-4">
            <FormField
              control={form.control}
              name="source_ref"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Repository</FormLabel>
                  <FormControl>
                    <Input placeholder="owner/repo" autoComplete="off" spellCheck={false} {...field} />
                  </FormControl>
                  <FormDescription>
                    <a
                      href={CREATE_LIBRARY_DOC_URL}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-pulse underline-offset-2 hover:underline focus-visible:underline"
                    >
                      Learn more<span className="sr-only"> about writing library entries (opens in a new tab)</span>
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
            <DialogFooter>
              <Button type="submit" disabled={pending}>
                {pending ? <Spinner size="sm" srLabel="Creating fleet library" /> : null}
                Create
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
