"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Spinner,
  Textarea,
} from "@agentsfleet/design-system";
import { installFleetAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

type Props = { workspaceId: string };

const schema = z.object({
  source_markdown: z.string().trim().min(1, "SKILL.md body is required").superRefine((value, ctx) => {
    const frontmatter = frontmatterBody(value);
    if (!frontmatter) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "SKILL.md needs frontmatter between --- markers",
      });
      return;
    }
    for (const field of ["name", "description", "version"]) {
      if (!hasTopLevelField(frontmatter, field)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `SKILL.md frontmatter needs ${field}:`,
        });
        return;
      }
    }
  }),
  trigger_markdown: z.string().trim().superRefine((value, ctx) => {
    if (value.length === 0) return;
    const frontmatter = frontmatterBody(value);
    if (!frontmatter) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "TRIGGER.md needs frontmatter between --- markers",
      });
      return;
    }
    if (!hasTopLevelField(frontmatter, "name")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "TRIGGER.md frontmatter needs name:",
      });
      return;
    }
    if (!hasTopLevelField(frontmatter, "x-agentsfleet")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "TRIGGER.md frontmatter needs x-agentsfleet:",
      });
      return;
    }
    for (const field of ["triggers", "tools", "budget"]) {
      if (!frontmatter.includes(`  ${field}:`)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `x-agentsfleet needs ${field}:`,
        });
        return;
      }
    }
  }),
});
type FormValues = z.infer<typeof schema>;

function frontmatterBody(markdown: string): string | null {
  const normalized = markdown.trimStart();
  if (!normalized.startsWith("---")) return null;
  const rest = normalized.slice(3);
  const end = rest.indexOf("\n---");
  if (end < 0) return null;
  return rest.slice(0, end);
}

function hasTopLevelField(frontmatter: string, field: string): boolean {
  return frontmatter
    .split(/\r?\n/)
    .some((line) => line.startsWith(`${field}:`));
}

// Server-side parsing stays the source of truth; the form only fills defaults.
export default function InstallFleetForm({ workspaceId }: Props) {
  const router = useRouter();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { source_markdown: "", trigger_markdown: "" },
  });

  function onSubmit(values: FormValues) {
    setApiError(null);
    const body = values.trigger_markdown
      ? values
      : { source_markdown: values.source_markdown };
    startTransition(async () => {
      const result = await installFleetAction(workspaceId, body);
      if (result.ok) {
        captureProductEvent(EVENTS.fleet_created, { fleet_id: result.data.fleet_id });
        router.push(`/fleets/${result.data.fleet_id}`);
        return;
      }
      if (result.status === 409) {
        setApiError("That teammate name already exists in this workspace.");
      } else {
        setApiError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "install the teammate",
          }),
        );
      }
    });
  }

  return (
    <Form {...form}>
      <form
        onSubmit={(e) => {
          void form.handleSubmit(onSubmit)(e);
        }}
        className="max-w-2xl space-y-5"
      >
        <ol aria-label="Install steps" className="grid gap-2 sm:grid-cols-2">
          <li className="flex gap-3 rounded-md border border-border bg-background px-3 py-2">
            <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-pulse text-xs font-semibold text-on-pulse">
              1
            </span>
            <div>
              <p className="text-sm font-medium text-foreground">Skill</p>
              <p className="text-xs text-muted-foreground">Required teammate behavior.</p>
            </div>
          </li>
          <li className="flex gap-3 rounded-md border border-border bg-background px-3 py-2">
            <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold text-muted-foreground">
              2
            </span>
            <div>
              <p className="text-sm font-medium text-foreground">Wake rule</p>
              <p className="text-xs text-muted-foreground">Optional; leave blank for manual wake.</p>
            </div>
          </li>
        </ol>

        <p className="rounded-md border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          Paste <code className="font-mono">SKILL.md</code> first.{" "}
          Leave <code className="font-mono">TRIGGER.md</code> blank and the server
          will install an API wake; add cron or webhook rules later.
        </p>
        <Alert variant="info">
          <div>
            <AlertTitle>What is SKILL.md?</AlertTitle>
            <AlertDescription>
              This is the teammate&apos;s operating guide: the goal, constraints,
              and steps it should follow. <code className="font-mono">agentsfleet</code>
              {" "}stores it with the installed teammate so every run wakes with
              the same instructions.
            </AlertDescription>
          </div>
        </Alert>

        <FormField
          control={form.control}
          name="source_markdown"
          render={({ field }) => (
            <FormItem>
              <FormLabel>SKILL.md body</FormLabel>
              <FormControl>
                <Textarea
                  placeholder={
                    "---\nname: my-teammate\ndescription: Automates the first run\nversion: 0.1.0\n---\n# My Teammate\n\nDescribe what the teammate should do."
                  }
                  rows={9}
                  className="font-mono text-xs"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                Step 1: teammate behavior and metadata. The{" "}
                <code className="font-mono">name</code> becomes the installed teammate name.
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
              <FormLabel>TRIGGER.md body</FormLabel>
              <FormControl>
                <Textarea
                  placeholder={
                    "---\nname: my-teammate\nx-agentsfleet:\n  triggers:\n    - type: cron\n      schedule: \"0 0 * * *\"\n  tools:\n    - agentmail\n  budget:\n    daily_dollars: 1.0\n---\n"
                  }
                  rows={8}
                  className="font-mono text-xs"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                Step 2: optional wake rules, tools, and budget. Leave blank for
                a generated API wake with no external tools. If pasted, include{" "}
                <code className="font-mono">x-agentsfleet.triggers</code>,
                {" "}<code className="font-mono">x-agentsfleet.tools</code>, and{" "}
                <code className="font-mono">x-agentsfleet.budget</code>.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />

        {apiError ? (
          <Alert variant="destructive">{apiError}</Alert>
        ) : null}

        <div className="flex gap-2 pt-2">
          <Button
            type="submit"
            disabled={pending}
            aria-busy={pending}
            variant="default"
            size="sm"
          >
            {pending ? <Spinner size="sm" label="Installing…" /> : "Install teammate"}
          </Button>
          <Button
            type="button"
            onClick={() => router.push("/fleets")}
            disabled={pending}
            variant="ghost"
            size="sm"
          >
            Cancel
          </Button>
        </div>
      </form>
    </Form>
  );
}
