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
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
  TooltipButton,
} from "@agentsfleet/design-system";
import { CircleHelpIcon, PlusIcon } from "lucide-react";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentError, type ErrorPresentation } from "@/lib/errors";
import { SOURCE_KIND_GITHUB, SOURCE_KIND_UPLOAD } from "@/lib/types";
import {
  SOURCE_REF_PATTERN,
  SAMPLE_LIBRARY_REPO,
  SAMPLE_LIBRARY_REPO_URL,
} from "@/lib/fleet-library-source";
import { onboardLibraryEntryAction } from "../actions";
import { CREATE_FLEET_LIBRARY_TOOLTIP, CREATE_LIBRARY_DOC_URL } from "./library-docs";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "./bundle-files";
import { BundleFolderPicker } from "./BundleFolderPicker";

const ONBOARD_ACTION = "create the fleet library";

const SKILL_REQUIRED = `Add the ${SKILL_FILE_NAME} body`;
const TRIGGER_REQUIRED = `Add the ${TRIGGER_FILE_NAME} body`;

// One flat shape rather than a discriminated union: react-hook-form registers
// fields by name, and a union whose branches carry different names leaves the
// inactive branch's inputs unregistered between tab switches. `source_kind`
// selects which fields are required instead.
const schema = z
  .object({
    source_kind: z.enum([SOURCE_KIND_GITHUB, SOURCE_KIND_UPLOAD]),
    source_ref: z.string().trim(),
    skill_markdown: z.string(),
    trigger_markdown: z.string(),
  })
  .superRefine((values, ctx) => {
    if (values.source_kind === SOURCE_KIND_GITHUB) {
      if (!SOURCE_REF_PATTERN.test(values.source_ref)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["source_ref"],
          message: `Use owner/repo, for example ${SAMPLE_LIBRARY_REPO}`,
        });
      }
      return;
    }
    // Both bodies are required together. A bundle uploaded without its
    // TRIGGER.md installs as a fleet that declares no tools, no credentials and
    // no gate — and the runtime reads an absent gate as "approve everything".
    if (values.skill_markdown.trim().length === 0) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["skill_markdown"], message: SKILL_REQUIRED });
    }
    if (values.trigger_markdown.trim().length === 0) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["trigger_markdown"], message: TRIGGER_REQUIRED });
    }
  });

type FormValues = z.infer<typeof schema>;

const EMPTY_FORM: FormValues = {
  source_kind: SOURCE_KIND_GITHUB,
  source_ref: "",
  skill_markdown: "",
  trigger_markdown: "",
};

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
    defaultValues: EMPTY_FORM,
  });
  const sourceKind = form.watch("source_kind");

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (next) return;
    requestIdRef.current += 1;
    setPending(false);
    setApiError(null);
    form.reset(EMPTY_FORM);
  }

  // A chosen folder fills the boxes rather than going straight to the wire.
  // Frontmatter is unforgiving here — a single apostrophe truncates the parse —
  // so the person uploading gets to read what leaves the browser.
  function handleBundleLoaded(skillMarkdown: string, triggerMarkdown: string) {
    form.setValue("skill_markdown", skillMarkdown);
    form.setValue("trigger_markdown", triggerMarkdown);
    form.clearErrors();
  }

  // Switching source clears the other tab's error state, so a half-filled
  // GitHub ref does not keep the upload tab's submit button explaining itself.
  function handleSourceChange(next: string) {
    setApiError(null);
    form.clearErrors();
    form.setValue("source_kind", next === SOURCE_KIND_UPLOAD ? SOURCE_KIND_UPLOAD : SOURCE_KIND_GITHUB);
  }

  async function onSubmit(values: FormValues) {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setApiError(null);
    setPending(true);
    // `POST /fleet-libraries` has always accepted an inline upload — it was the
    // dashboard that only ever spoke `github`, which is why hand-setup installed
    // some other entry and overwrote both of its markdown files afterwards.
    const payload =
      values.source_kind === SOURCE_KIND_UPLOAD
        ? {
            source_kind: SOURCE_KIND_UPLOAD,
            skill_markdown: values.skill_markdown,
            trigger_markdown: values.trigger_markdown,
          }
        : { source_kind: SOURCE_KIND_GITHUB, source_ref: values.source_ref };
    try {
      const result = await onboardLibraryEntryAction(workspaceId, payload);
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
        <TooltipButton type="button" size="sm" tooltip={CREATE_FLEET_LIBRARY_TOOLTIP}>
          <PlusIcon size={14} />
          {triggerLabel}
        </TooltipButton>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create fleet library</DialogTitle>
          <DialogDescription>
            Create from a GitHub repository that contains a fleet library entry, or from a
            bundle directory on this machine.
          </DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }} className="space-y-4">
            <Tabs value={sourceKind} onValueChange={handleSourceChange}>
              <TabsList>
                <TabsTrigger value={SOURCE_KIND_GITHUB}>GitHub</TabsTrigger>
                <TabsTrigger value={SOURCE_KIND_UPLOAD}>Local folder</TabsTrigger>
              </TabsList>
              <TabsContent value={SOURCE_KIND_UPLOAD} className="space-y-4">
                <BundleFolderPicker onLoaded={handleBundleLoaded} />
                <FormField
                  control={form.control}
                  name="skill_markdown"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{SKILL_FILE_NAME}</FormLabel>
                      <FormControl>
                        <Textarea rows={8} spellCheck={false} placeholder="---&#10;name: my-fleet&#10;---" {...field} />
                      </FormControl>
                      <FormDescription>
                        The entry is named from this frontmatter, and TRIGGER.md below must name
                        the same fleet.
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="trigger_markdown"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{TRIGGER_FILE_NAME}</FormLabel>
                      <FormControl>
                        <Textarea rows={8} spellCheck={false} placeholder="---&#10;name: my-fleet&#10;x-agentsfleet:&#10;  triggers:&#10;---" {...field} />
                      </FormControl>
                      <FormDescription>
                        Declares the tools, credentials, hosts and approval gates this fleet runs
                        under. Uploading without it yields a fleet that declares none of them.
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </TabsContent>
              <TabsContent value={SOURCE_KIND_GITHUB}>
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
                    <span className="block">
                      Example:{" "}
                      <a
                        href={SAMPLE_LIBRARY_REPO_URL}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-pulse underline-offset-2 hover:underline focus-visible:underline"
                      >
                        {SAMPLE_LIBRARY_REPO}
                        <span className="sr-only"> (opens in a new tab)</span>
                      </a>
                    </span>
                    <a
                      href={CREATE_LIBRARY_DOC_URL}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-flex items-center gap-1 text-pulse underline-offset-2 hover:underline focus-visible:underline"
                    >
                      <CircleHelpIcon size={13} aria-hidden="true" />
                      Learn more<span className="sr-only"> about writing library entries (opens in a new tab)</span>
                    </a>
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />
              </TabsContent>
            </Tabs>
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
                {pending ? <Spinner size="sm" srLabel="Creating fleet library" /> : null}
                Create
              </TooltipButton>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
