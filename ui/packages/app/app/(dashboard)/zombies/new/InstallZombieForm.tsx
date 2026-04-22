"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@clerk/nextjs";
import { Loader2Icon } from "lucide-react";
import { buttonClassName } from "@usezombie/design-system";
import { installZombie } from "@/lib/api/zombies";

type Props = { workspaceId: string };

// Power-user install. Server contract is {name, source_markdown, config_json}.
// The ergonomic flow is `zombiectl up`, which reads SKILL.md + TRIGGER.md
// from disk and compiles the config JSON. This form exists so an operator
// without CLI access can still install by pasting the two bodies directly.
export default function InstallZombieForm({ workspaceId }: Props) {
  const router = useRouter();
  const { getToken } = useAuth();
  const [name, setName] = useState("");
  const [sourceMarkdown, setSourceMarkdown] = useState("");
  const [configJson, setConfigJson] = useState("");
  const [fieldError, setFieldError] = useState<string | null>(null);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setFieldError(null);
    setApiError(null);

    if (!name.trim()) {
      setFieldError("Zombie name is required");
      return;
    }
    if (!sourceMarkdown.trim()) {
      setFieldError("SKILL.md body is required");
      return;
    }
    if (!configJson.trim()) {
      setFieldError("Config JSON is required");
      return;
    }
    try {
      JSON.parse(configJson);
    } catch {
      setFieldError("Config JSON must be valid JSON");
      return;
    }

    const token = await getToken();
    if (!token) {
      setApiError("Not authenticated");
      return;
    }

    startTransition(async () => {
      try {
        const created = await installZombie(
          workspaceId,
          {
            name: name.trim(),
            source_markdown: sourceMarkdown,
            config_json: configJson,
          },
          token,
        );
        router.push(`/zombies/${created.zombie_id}`);
        router.refresh();
      } catch (e) {
        const err = e as Error & { status?: number };
        if (err.status === 409) {
          setApiError(
            `A zombie named "${name}" already exists in this workspace`,
          );
        } else {
          setApiError(err.message || "Install failed");
        }
      }
    });
  }

  return (
    <form onSubmit={onSubmit} className="max-w-xl space-y-4">
      <p className="rounded-md border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        Power-user install. Prefer{" "}
        <code className="font-mono">zombiectl up</code>, which reads SKILL.md
        and TRIGGER.md from disk and compiles the config JSON automatically.
      </p>

      <div>
        <label htmlFor="name" className="mb-1 block text-sm font-medium">
          Name
        </label>
        <input
          id="name"
          name="name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="lead-collector"
          className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
        />
      </div>

      <div>
        <label
          htmlFor="source_markdown"
          className="mb-1 block text-sm font-medium"
        >
          SKILL.md body
        </label>
        <textarea
          id="source_markdown"
          name="source_markdown"
          value={sourceMarkdown}
          onChange={(e) => setSourceMarkdown(e.target.value)}
          placeholder={"---\nname: lead-collector\n---\n# Lead Collector\n..."}
          rows={8}
          className="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-xs"
        />
        <p className="mt-1 text-xs text-muted-foreground">
          Paste the full contents of your skill{"'"}s{" "}
          <code className="font-mono">SKILL.md</code> file.
        </p>
      </div>

      <div>
        <label
          htmlFor="config_json"
          className="mb-1 block text-sm font-medium"
        >
          Config JSON
        </label>
        <textarea
          id="config_json"
          name="config_json"
          value={configJson}
          onChange={(e) => setConfigJson(e.target.value)}
          placeholder={'{"name":"lead-collector","trigger":{"kind":"webhook"}}'}
          rows={6}
          className="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-xs"
        />
        <p className="mt-1 text-xs text-muted-foreground">
          Compiled from TRIGGER.md frontmatter. Operators typically use the
          CLI to generate this.
        </p>
      </div>

      {fieldError ? (
        <div role="alert" className="text-sm text-destructive">
          {fieldError}
        </div>
      ) : null}

      {apiError ? (
        <div
          role="alert"
          className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive"
        >
          {apiError}
        </div>
      ) : null}

      <div className="flex gap-2 pt-2">
        <button
          type="submit"
          disabled={pending}
          aria-busy={pending}
          className={buttonClassName("default", "sm")}
        >
          {pending ? (
            <>
              <Loader2Icon size={14} className="animate-spin" aria-hidden="true" />
              Installing…
            </>
          ) : (
            "Install Zombie"
          )}
        </button>
        <button
          type="button"
          onClick={() => router.push("/zombies")}
          disabled={pending}
          className={buttonClassName("ghost", "sm")}
        >
          Cancel
        </button>
      </div>
    </form>
  );
}
