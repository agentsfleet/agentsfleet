"use client";

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
  Textarea,
} from "@agentsfleet/design-system";

// Paste-create input. Validates the SKILL.md (and optional TRIGGER.md)
// frontmatter client-side, then hands the validated markdown to the install
// states via `onSubmit` — it does NOT post or route. Create runs inline in the
// states so paste stays on the same one-experience path as templates/GitHub;
// the malformed-paste guard below blocks create before it can reach the server.
type Props = {
  onBack: () => void;
  onSubmit: (sourceMarkdown: string, triggerMarkdown?: string) => void;
};

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
export default function InstallFleetForm({ onBack, onSubmit }: Props) {
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { source_markdown: "", trigger_markdown: "" },
  });

  function submit(values: FormValues) {
    onSubmit(
      values.source_markdown,
      values.trigger_markdown.length > 0 ? values.trigger_markdown : undefined,
    );
  }

  return (
    <div className="space-y-4">
      <Button type="button" variant="ghost" size="sm" onClick={onBack}>
        ← Back to templates
      </Button>
      <Form {...form}>
        <form
          onSubmit={(e) => {
            void form.handleSubmit(submit)(e);
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
                <p className="text-xs text-muted-foreground">Required fleet behavior.</p>
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
            Leave <code className="font-mono">TRIGGER.md</code> blank for a manual API wake.
          </p>
          <Alert variant="info">
            <div>
              <AlertTitle>What is SKILL.md?</AlertTitle>
              <AlertDescription>
                The fleet guide. <code className="font-mono">agentsfleet</code>{" "}
                stores it for every run.
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
                      "---\nname: my-fleet\ndescription: Automates the first run\nversion: 0.1.0\n---\n# My Fleet\n\nDescribe what the fleet should do."
                    }
                    rows={9}
                    className="font-mono text-xs"
                    {...field}
                  />
                </FormControl>
                <FormDescription>
                  Step 1: fleet behavior and metadata. The{" "}
                  <code className="font-mono">name</code> becomes the installed fleet name.
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
                      "---\nname: my-fleet\nx-agentsfleet:\n  triggers:\n    - type: cron\n      schedule: \"0 0 * * *\"\n  tools:\n    - agentmail\n  budget:\n    daily_dollars: 1.0\n---\n"
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

          <div className="flex gap-2 pt-2">
            <Button type="submit" variant="default" size="sm">
              Create fleet
            </Button>
            <Button type="button" onClick={onBack} variant="ghost" size="sm">
              Cancel
            </Button>
          </div>
        </form>
      </Form>
    </div>
  );
}
