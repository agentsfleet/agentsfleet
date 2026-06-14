"use client";

import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Spinner,
} from "@agentsfleet/design-system";
import {
  HOST_ID_REGEX,
  SANDBOX_TIERS,
  parseLabels,
  type CreatedRunner,
  type SandboxTier,
} from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { createRunnerAction } from "../actions";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const DEFAULT_TIER: SandboxTier = "landlock_full";

const schema = z.object({
  host_id: z.string().trim().regex(HOST_ID_REGEX, "1–256 characters: letters, digits, dot, hyphen, underscore"),
  sandbox_tier: z.enum(SANDBOX_TIERS),
  labels: z.string().trim(),
});
type FormValues = z.infer<typeof schema>;

export default function AddRunnerDialog({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [created, setCreated] = useState<CreatedRunner | null>(null);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { host_id: "", sandbox_tier: DEFAULT_TIER, labels: "" },
  });

  // Single dismissal path. Outside-click / Escape are locked during reveal (see
  // DialogContent), so this fires only from the X or the explicit button.
  // Discarding `created` drops the raw zrn_ from React state → out of the DOM.
  function handleOpenChange(next: boolean) {
    if (next) {
      setOpen(true);
      return;
    }
    const minted = created !== null;
    setOpen(false);
    setCreated(null);
    setApiError(null);
    form.reset({ host_id: "", sandbox_tier: DEFAULT_TIER, labels: "" });
    if (minted) onCreated();
  }

  function onSubmit(values: FormValues) {
    setApiError(null);
    const parsed = parseLabels(values.labels);
    if (parsed.error) {
      setApiError(parsed.error);
      return;
    }
    startTransition(async () => {
      const r = await createRunnerAction({
        host_id: values.host_id.trim(),
        sandbox_tier: values.sandbox_tier,
        labels: parsed.labels,
      });
      if (!r.ok) {
        setApiError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "enroll the runner" }));
        return;
      }
      // Reveal first, capture second — the one-time token must render even if
      // analytics misbehaves.
      setCreated(r.data);
      captureProductEvent(EVENTS.runner_token_minted, {
        runner_id: r.data.runner_id,
        sandbox_tier: values.sandbox_tier,
      });
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button type="button" size="sm">
          Add runner
        </Button>
      </DialogTrigger>
      <DialogContent
        onInteractOutside={(e) => {
          if (created) e.preventDefault();
        }}
        onEscapeKeyDown={(e) => {
          if (created) e.preventDefault();
        }}
      >
        {created ? (
          <RevealPanel token={created.runner_token} onClose={() => handleOpenChange(false)} />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Add runner</DialogTitle>
              <DialogDescription>
                The runner token is shown once. Name the host so you can recognise it in the fleet.
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
                  name="host_id"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Host id</FormLabel>
                      <FormControl>
                        <Input placeholder="web-prod-1" autoComplete="off" {...field} />
                      </FormControl>
                      <FormDescription>A stable identifier for the host.</FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="sandbox_tier"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Sandbox tier</FormLabel>
                      <Select value={field.value} onValueChange={field.onChange}>
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {SANDBOX_TIERS.map((t) => (
                            <SelectItem key={t} value={t}>
                              {t}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <FormDescription>The host's isolation strength.</FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="labels"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Labels (optional)</FormLabel>
                      <FormControl>
                        <Input placeholder="gpu, us-east" autoComplete="off" {...field} />
                      </FormControl>
                      <FormDescription>Comma-separated capability labels.</FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                {apiError ? <p className="text-sm text-destructive">{apiError}</p> : null}
                <DialogFooter>
                  <Button type="submit" disabled={pending}>
                    {pending ? <Spinner size="sm" srLabel="Enrolling" /> : null}
                    Create runner
                  </Button>
                </DialogFooter>
              </form>
            </Form>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function RevealPanel({ token, onClose }: { token: string; onClose: () => void }) {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");

  async function copy() {
    try {
      await navigator.clipboard.writeText(token);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

  return (
    <>
      <DialogHeader>
        <DialogTitle>Save the runner token</DialogTitle>
        <DialogDescription>
          This is the only time it is shown. Install it on the host as <span className="font-mono">AGENTSFLEET_RUNNER_TOKEN</span>{" "}
          — you won&apos;t be able to see it again.
        </DialogDescription>
      </DialogHeader>
      {/* ph-no-capture keeps the one-time raw token out of PostHog autocapture
          and session replay, even if input masking is relaxed project-side. */}
      <div className="space-y-3 ph-no-capture">
        <Input
          readOnly
          value={token}
          aria-label="Runner token"
          className="font-mono text-sm"
          onFocus={(e) => e.currentTarget.select()}
        />
        <Button type="button" variant="ghost" size="sm" onClick={() => void copy()}>
          {copyState === "copied" ? "Copied" : "Copy to clipboard"}
        </Button>
        {copyState === "failed" ? (
          <p className="text-sm text-destructive">Copy failed — select the value above and copy it manually.</p>
        ) : null}
      </div>
      <DialogFooter>
        <Button type="button" onClick={onClose}>
          I&apos;ve stored it — close
        </Button>
      </DialogFooter>
    </>
  );
}
