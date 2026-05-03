"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useClientToken } from "@/lib/auth/client";
import { Loader2Icon } from "lucide-react";
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
  Input,
  Textarea,
} from "@usezombie/design-system";
import { installZombie } from "@/lib/api/zombies";

type Props = { workspaceId: string };

const schema = z.object({
  name: z.string().trim().min(1, "Zombie name is required"),
  source_markdown: z.string().trim().min(1, "SKILL.md body is required"),
  config_json: z
    .string()
    .trim()
    .min(1, "Config JSON is required")
    .refine((s) => {
      try {
        JSON.parse(s);
        return true;
      } catch {
        return false;
      }
    }, "Config JSON must be valid JSON"),
});
type FormValues = z.infer<typeof schema>;

// Power-user install. Server contract is {name, source_markdown, config_json}.
// The ergonomic flow is `zombiectl up`, which reads SKILL.md + TRIGGER.md
// from disk and compiles the config JSON. This form exists so an operator
// without CLI access can still install by pasting the two bodies directly.
export default function InstallZombieForm({ workspaceId }: Props) {
  const router = useRouter();
  const { getToken } = useClientToken();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: "", source_markdown: "", config_json: "" },
  });

  function onSubmit(values: FormValues) {
    setApiError(null);
    startTransition(async () => {
      const token = await getToken();
      if (!token) {
        setApiError("Not authenticated");
        return;
      }
      try {
        const created = await installZombie(workspaceId, values, token);
        router.push(`/zombies/${created.zombie_id}`);
        router.refresh();
      } catch (e) {
        const err = e as Error & { status?: number };
        if (err.status === 409) {
          setApiError(`A zombie named "${values.name}" already exists in this workspace`);
        } else {
          setApiError(err.message || "Install failed");
        }
      }
    });
  }

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)} className="max-w-xl space-y-4">
        <p className="rounded-md border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          Power-user install. Prefer{" "}
          <code className="font-mono">zombiectl up</code>, which reads SKILL.md
          and TRIGGER.md from disk and compiles the config JSON automatically.
        </p>

        <FormField
          control={form.control}
          name="name"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Name</FormLabel>
              <FormControl>
                <Input placeholder="lead-collector" {...field} />
              </FormControl>
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
                  placeholder={"---\nname: lead-collector\n---\n# Lead Collector\n..."}
                  rows={8}
                  className="font-mono text-xs"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                Paste the full contents of your skill&apos;s{" "}
                <code className="font-mono">SKILL.md</code> file.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="config_json"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Config JSON</FormLabel>
              <FormControl>
                <Textarea
                  placeholder={'{"name":"lead-collector","trigger":{"kind":"webhook"}}'}
                  rows={6}
                  className="font-mono text-xs"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                Compiled from TRIGGER.md frontmatter. Operators typically use
                the CLI to generate this.
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
            {pending ? (
              <>
                <Loader2Icon size={14} className="animate-spin" aria-hidden="true" />
                Installing…
              </>
            ) : (
              "Install Zombie"
            )}
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
