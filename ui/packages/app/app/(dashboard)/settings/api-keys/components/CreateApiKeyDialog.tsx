"use client";

import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Button,
  CopyButton,
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
import { AlertTriangleIcon, PlusIcon } from "lucide-react";
import { KEY_NAME_REGEX, DESCRIPTION_MAX, type CreatedApiKey } from "@/lib/api/api_keys";
import { presentErrorString } from "@/lib/errors";
import { createApiKeyAction } from "../actions";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const schema = z.object({
  key_name: z
    .string()
    .trim()
    .regex(KEY_NAME_REGEX, "1–64 characters: letters, digits, hyphen, underscore only"),
  description: z.string().trim().max(DESCRIPTION_MAX, `Description must be ${DESCRIPTION_MAX} characters or fewer`),
});
type FormValues = z.infer<typeof schema>;
const CREATE_KEY_TOOLTIP = "Create an agentsfleet API key.";

export default function CreateApiKeyDialog({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [created, setCreated] = useState<CreatedApiKey | null>(null);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: { key_name: "", description: "" } });

  // Single dismissal path. Outside-click / Escape are locked during reveal
  // (see DialogContent), so this fires only from the X or the explicit button.
  // Discarding `created` drops the raw key from React state → out of the DOM.
  function handleOpenChange(next: boolean) {
    if (next) {
      setOpen(true);
      return;
    }
    const minted = created !== null;
    setOpen(false);
    setCreated(null);
    setApiError(null);
    form.reset({ key_name: "", description: "" });
    if (minted) onCreated();
  }

  function onSubmit(values: FormValues) {
    setApiError(null);
    startTransition(async () => {
      const r = await createApiKeyAction({
        key_name: values.key_name.trim(),
        description: values.description.trim() || undefined,
      });
      if (!r.ok) {
        setApiError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "create the API key" }));
        return;
      }
      // Reveal first, capture second — the one-time key must render even if
      // analytics misbehaves.
      setCreated(r.data);
      captureProductEvent(EVENTS.api_key_minted, { api_key_id: r.data.id });
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <TooltipButton type="button" size="sm" tooltip={CREATE_KEY_TOOLTIP}>
          <PlusIcon size={14} />
          Create key
        </TooltipButton>
      </DialogTrigger>
      <DialogContent
        onInteractOutside={(e) => { if (created) e.preventDefault(); }}
        onEscapeKeyDown={(e) => { if (created) e.preventDefault(); }}
      >
        {created ? (
          <RevealPanel keyValue={created.key} onClose={() => handleOpenChange(false)} />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Create API key</DialogTitle>
              <DialogDescription>
                Name it so you can find it later.
              </DialogDescription>
            </DialogHeader>
            <Form {...form}>
              <form onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }} className="space-y-4">
                <FormField
                  control={form.control}
                  name="key_name"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Name</FormLabel>
                      <FormControl>
                        <Input placeholder="ci-runner" autoComplete="off" {...field} />
                      </FormControl>
                      <FormDescription>Unique within your tenant.</FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="description"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Description (optional)</FormLabel>
                      <FormControl>
                        <Input placeholder="What uses this key?" autoComplete="off" {...field} />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                {apiError ? <p className="text-sm text-destructive">{apiError}</p> : null}
                <DialogFooter>
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={pending}
                    onClick={() => handleOpenChange(false)}
                  >
                    Cancel
                  </Button>
                  <TooltipButton type="submit" disabled={pending} tooltip={CREATE_KEY_TOOLTIP}>
                    {pending ? <Spinner size="sm" srLabel="Creating" /> : null}
                    Create key
                  </TooltipButton>
                </DialogFooter>
              </form>
            </Form>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function RevealPanel({ keyValue, onClose }: { keyValue: string; onClose: () => void }) {
  // Sticky once true — see AddRunnerDialog: a one-time value's failure notice
  // must outlive the button's 2s flash.
  const [copyEverFailed, setCopyEverFailed] = useState(false);
  return (
    <>
      <DialogHeader>
        <DialogTitle>Save your API key</DialogTitle>
        <DialogDescription className="flex items-center gap-1.5 text-warning">
          <AlertTriangleIcon size={14} className="shrink-0" aria-hidden />
          API key is shown once. Copy it now.
        </DialogDescription>
      </DialogHeader>
      {/* ph-no-capture keeps the one-time raw key out of PostHog autocapture and
          session replay, even if input masking is relaxed project-side later. */}
      <div className="space-y-3 ph-no-capture">
        {/* The copy sits ON the field. This key is shown once and cannot be
            recovered, so a silent clipboard failure would cost the user the key —
            CopyButton reports one rather than swallowing it. */}
        <div className="flex items-center gap-2">
          <Input
            readOnly
            value={keyValue}
            aria-label="API key value"
            className="font-mono text-sm"
            onFocus={(e) => e.currentTarget.select()}
          />
          <CopyButton
            value={keyValue}
            label="Copy API key"
            onOutcomeChange={(outcome) => {
              if (outcome === "failed") setCopyEverFailed(true);
            }}
          />
        </div>
        {copyEverFailed ? (
          <p className="text-sm text-destructive">
            Copy failed — select the key above and copy it manually.
          </p>
        ) : null}
      </div>
      <DialogFooter>
        <Button type="button" onClick={onClose}>
          Done
        </Button>
      </DialogFooter>
    </>
  );
}
