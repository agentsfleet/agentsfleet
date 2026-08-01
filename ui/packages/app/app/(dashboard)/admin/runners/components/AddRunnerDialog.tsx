"use client";

import { useState, useTransition } from "react";
import { useForm, type Control } from "react-hook-form";
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
import { InfoIcon, PlusIcon } from "lucide-react";
import { HOST_ID_REGEX, parseLabels, type CreatedRunner } from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { createRunnerAction } from "../actions";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import {
  POLICY_FORM_DEFAULTS,
  PolicyFields,
  policyFormSchema,
  policyFromForm,
  type PolicyFormValues,
} from "./PolicyFields";

const RUNNER_TOKEN_WARNING = "Runner token is shown once. Copy it now.";
const CREATE_RUNNER_TOOLTIP = "Enroll a host to run fleets.";
// The selection is an assignment the host must satisfy — never a description
// of the host (Dimension 4.3); the per-field copy in PolicyFields agrees.
const CREATE_RUNNER_DESCRIPTION =
  "A runner is a host you enroll to run fleet work. The policy you pick here is assigned to the host — it applies exactly this, and reports what it can actually enforce.";

const schema = policyFormSchema.extend({
  host_id: z.string().trim().regex(HOST_ID_REGEX, "1–256 characters: letters, digits, dot, hyphen, underscore"),
  labels: z.string().trim(),
});
type FormValues = z.infer<typeof schema>;

const FORM_DEFAULTS: FormValues = { host_id: "", labels: "", ...POLICY_FORM_DEFAULTS };

export default function AddRunnerDialog({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [created, setCreated] = useState<CreatedRunner | null>(null);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: FORM_DEFAULTS,
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
    form.reset(FORM_DEFAULTS);
    if (minted) onCreated();
  }

  function onSubmit(values: FormValues) {
    setApiError(null);
    const parsedLabels = parseLabels(values.labels);
    if (parsedLabels.error) {
      setApiError(parsedLabels.error);
      return;
    }
    const assignment = policyFromForm(values);
    if (assignment.error !== null || assignment.policy === null) {
      setApiError(assignment.error);
      return;
    }
    const assigned_policy = assignment.policy;
    startTransition(async () => {
      const r = await createRunnerAction({
        host_id: values.host_id.trim(),
        assigned_policy,
        labels: parsedLabels.labels,
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
        sandbox_tier: assigned_policy.sandbox_tier,
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
      {/* Same policy form as EditPolicyDialog, plus the post-create token panel —
          so it needs the same bounded height and scroll to keep its actions
          reachable on a short viewport. */}
      <DialogContent
        className="max-h-svh overflow-y-auto"
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
              <DialogDescription>{CREATE_RUNNER_DESCRIPTION}</DialogDescription>
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
                {/* One boundary cast: FormValues extends PolicyFormValues, but
                    react-hook-form's Control generic is invariant. */}
                <PolicyFields control={form.control as unknown as Control<PolicyFormValues>} />
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
