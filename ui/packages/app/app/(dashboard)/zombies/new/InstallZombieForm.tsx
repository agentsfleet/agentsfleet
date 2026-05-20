"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Alert,
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
} from "@usezombie/design-system";
import { installZombieAction } from "../actions";
import { presentErrorString } from "@/lib/errors";

type Props = { workspaceId: string };

const schema = z.object({
  trigger_markdown: z.string().trim().min(1, "TRIGGER.md body is required"),
  source_markdown: z.string().trim().min(1, "SKILL.md body is required"),
});
type FormValues = z.infer<typeof schema>;

// Mirrors `zombiectl install --from`: paste TRIGGER.md + SKILL.md, zombied
// parses the YAML frontmatter and derives name + config from it. Same wire
// contract as the CLI — no client-side compile, no hand-crafted JSON.
export default function InstallZombieForm({ workspaceId }: Props) {
  const router = useRouter();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { trigger_markdown: "", source_markdown: "" },
  });

  function onSubmit(values: FormValues) {
    setApiError(null);
    startTransition(async () => {
      const result = await installZombieAction(workspaceId, values);
      if (result.ok) {
        // No router.refresh() — calling refresh immediately after push races
        // inside the same transition: refresh re-fetches the *current* route
        // (/zombies/new) before the push URL commits, leaving the browser
        // stuck on the form even though Next fetched the destination
        // React Server Component tree.
        // The destination page fetches its own data; no refresh needed.
        router.push(`/zombies/${result.data.zombie_id}`);
        return;
      }
      // Conflict gets a hand-rolled message — 409 here always means name
      // collision on the workspace, and pointing the operator at the
      // exact field to fix beats any generic wording. Everything else
      // routes through presentError so the surfaced server message is
      // wrapped in our voice instead of "Failed to <verb>".
      if (result.status === 409) {
        setApiError(
          `That bundle's name already exists in this workspace — change the \`name:\` in TRIGGER.md frontmatter.`,
        );
      } else {
        setApiError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "install the zombie",
          }),
        );
      }
    });
  }

  return (
    <Form {...form}>
      <form onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }} className="max-w-xl space-y-4">
        <p className="rounded-md border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          Paste the two markdown files that <code className="font-mono">zombiectl install --from</code>
          {" "}reads from disk. <code className="font-mono">name</code> and the compiled config are
          derived from <code className="font-mono">TRIGGER.md</code> frontmatter server-side — same
          wire as the CLI.
        </p>

        <FormField
          control={form.control}
          name="trigger_markdown"
          render={({ field }) => (
            <FormItem>
              <FormLabel>TRIGGER.md body</FormLabel>
              <FormControl>
                <Textarea
                  placeholder={
                    "---\nname: my-zombie\nx-usezombie:\n  trigger:\n    type: api\n  tools:\n    - agentmail\n  budget:\n    daily_dollars: 1.0\n---\n"
                  }
                  rows={8}
                  className="font-mono text-xs"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                YAML frontmatter must include <code className="font-mono">name</code> (kebab-case),
                {" "}<code className="font-mono">x-usezombie.trigger</code>,{" "}
                <code className="font-mono">x-usezombie.tools</code>, and{" "}
                <code className="font-mono">x-usezombie.budget</code>.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="source_markdown"
          render={({ field }) => (
            <FormItem>
              <FormLabel>SKILL.md body</FormLabel>
              <FormControl>
                <Textarea
                  placeholder={"---\nname: my-zombie\ndescription: …\nversion: 0.1.0\n---\n# My Zombie\n…"}
                  rows={8}
                  className="font-mono text-xs"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                Paste the full contents of the skill&apos;s{" "}
                <code className="font-mono">SKILL.md</code> file.
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
            {pending ? <Spinner size="sm" label="Installing…" /> : "Install Zombie"}
          </Button>
          <Button
            type="button"
            onClick={() => router.push("/zombies")}
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
