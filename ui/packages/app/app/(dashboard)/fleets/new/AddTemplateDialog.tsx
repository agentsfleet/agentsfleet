"use client";

import { useState } from "react";
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
import { onboardTemplateAction } from "../actions";

const SOURCE_REF_PATTERN = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
const CREATE_TEMPLATE_DOC_URL = "https://docs.agentsfleet.net/fleets/templates#writing-your-own";
const ONBOARD_ACTION = "add the template";

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
};

export default function AddTemplateDialog({ workspaceId, triggerLabel = "Add template" }: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [apiError, setApiError] = useState<ErrorPresentation | null>(null);
  const [pending, setPending] = useState(false);
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { source_ref: "" },
  });

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (next) return;
    setApiError(null);
    form.reset({ source_ref: "" });
  }

  async function onSubmit(values: FormValues) {
    setApiError(null);
    setPending(true);
    const sourceRef = values.source_ref.trim();
    try {
      const result = await onboardTemplateAction(workspaceId, {
        source_kind: SOURCE_KIND_GITHUB,
        source_ref: sourceRef,
      });
      if (!result.ok) {
        setApiError(presentError({
          errorCode: result.errorCode,
          message: result.error,
          action: ONBOARD_ACTION,
        }));
        return;
      }
      captureProductEvent(EVENTS.fleet_template_onboarded, {
        workspace_id: workspaceId,
        visibility: result.data.visibility,
        source_kind: SOURCE_KIND_GITHUB,
        outcome: "success",
      });
      handleOpenChange(false);
      router.refresh();
    } finally {
      setPending(false);
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
          <DialogTitle>Add template</DialogTitle>
          <DialogDescription>
            Add a GitHub repository that contains a Fleet template.
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
                      href={CREATE_TEMPLATE_DOC_URL}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-pulse underline-offset-2 hover:underline focus-visible:underline"
                    >
                      Create a template
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
                {pending ? <Spinner size="sm" srLabel="Adding template" /> : null}
                Add template
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

export { CREATE_TEMPLATE_DOC_URL };
