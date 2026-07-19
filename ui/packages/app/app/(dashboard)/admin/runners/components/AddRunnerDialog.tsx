"use client";

import { useId, useState, useTransition } from "react";
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
  OptionCard,
  RadioGroup,
  Spinner,
  TooltipButton,
} from "@agentsfleet/design-system";
import { InfoIcon, PlusIcon } from "lucide-react";
import {
  HOST_ID_REGEX,
  SANDBOX_TIERS,
  SANDBOX_TIER_DESCRIPTIONS,
  SANDBOX_TIER_LABELS,
  parseLabels,
  type CreatedRunner,
  type SandboxTier,
} from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { createRunnerAction } from "../actions";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const DEFAULT_TIER: SandboxTier = "landlock_full";
const RUNNER_TOKEN_WARNING = "Runner token is shown once. Copy it now.";
const CREATE_RUNNER_TOOLTIP = "Enroll a host to run fleets.";

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
  const isolationModeLabelId = useId();
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { host_id: "", sandbox_tier: DEFAULT_TIER, labels: "" },
  });

  // Single dismissal path. Outside-click / Escape are locked during reveal (see
  // DialogContent), so this fires only from the X or the explicit button.
  // Discarding `created` drops the raw agt_r from React state → out of the DOM.
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
        <TooltipButton type="button" size="sm" tooltip={CREATE_RUNNER_TOOLTIP}>
          <PlusIcon size={14} />
          Create runner
        </TooltipButton>
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
          // The reveal is INLINE — never a child component taking the token as a prop.
          //
          // A one-time runner token is never handed to another component as a prop.
          // `tests/grep-gates/no-api-template-mint.test.ts` forbids a token-typed
          // prop in any "use client" file, because a prop crossing into a client
          // component is serialized into the hydration payload. Here `created` is
          // already client state from a server action, so the prop was not in fact
          // a hydration leak — but a regex cannot know that, and a credential that
          // is never named as a prop cannot become one when the next person moves
          // this panel into its own file. The gate wins; the token stays put.
          <>
            <DialogHeader>
              <DialogTitle>Save the runner token</DialogTitle>
              <DialogDescription className="flex items-center gap-1.5 text-warning">
                <InfoIcon size={14} className="shrink-0" aria-hidden />
                {RUNNER_TOKEN_WARNING}
              </DialogDescription>
            </DialogHeader>
            {/* ph-no-capture keeps the one-time raw token out of PostHog autocapture
                and session replay, even if input masking is relaxed project-side. */}
            <div className="space-y-3 ph-no-capture">
              <p className="text-sm text-muted-foreground">
                Install it on the host as <span className="font-mono">AGENTSFLEET_RUNNER_TOKEN</span>.
              </p>
              {/* The copy sits ON the field, not below it. This value is shown once
                  and cannot be recovered, so the affordance belongs where the eye
                  already is. CopyButton reports a failed write rather than swallowing
                  it — a silent failure here costs the operator the token for good. */}
              <div className="flex items-center gap-2">
                <Input
                  readOnly
                  value={created.runner_token}
                  aria-label="Runner token"
                  className="font-mono text-sm"
                  onFocus={(e) => e.currentTarget.select()}
                />
                <CopyButton value={created.runner_token} label="Copy runner token" />
              </div>
            </div>
            <DialogFooter>
              <Button type="button" onClick={() => handleOpenChange(false)}>
                I&apos;ve stored it — close
              </Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Create runner</DialogTitle>
              <DialogDescription>
                A runner is a host you enroll to run fleet work.
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
                      <FormLabel>Host name</FormLabel>
                      <FormControl>
                        <Input placeholder="web-prod-1" autoComplete="off" {...field} />
                      </FormControl>
                      <FormDescription>A name to recognise this host in the list.</FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="sandbox_tier"
                  render={({ field }) => (
                    <FormItem>
                      {/* RadioGroup's root renders a <div role="radiogroup">, not a
                          labelable HTML element — FormLabel's htmlFor (built for a
                          single input/button/select) can't auto-focus it, so the
                          group is named directly via aria-labelledby instead. */}
                      <FormLabel id={isolationModeLabelId}>Isolation mode</FormLabel>
                      <FormControl>
                        <RadioGroup
                          value={field.value}
                          onValueChange={field.onChange}
                          aria-labelledby={isolationModeLabelId}
                          className="sm:grid-cols-2"
                        >
                          {SANDBOX_TIERS.map((t) => (
                            <OptionCard
                              key={t}
                              value={t}
                              label={SANDBOX_TIER_LABELS[t]}
                              description={SANDBOX_TIER_DESCRIPTIONS[t]}
                            />
                          ))}
                        </RadioGroup>
                      </FormControl>
                      <FormDescription>How the host isolates fleet work — self-reported.</FormDescription>
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
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={pending}
                    onClick={() => handleOpenChange(false)}
                  >
                    Cancel
                  </Button>
                  <TooltipButton type="submit" disabled={pending} tooltip={CREATE_RUNNER_TOOLTIP}>
                    {pending ? <Spinner size="sm" srLabel="Enrolling" /> : null}
                    Create runner
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
