"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@clerk/nextjs";
import { Loader2Icon } from "lucide-react";
import { buttonClassName } from "@usezombie/design-system";
import { installZombie } from "@/lib/api/zombies";

type Props = { workspaceId: string };

export default function InstallZombieForm({ workspaceId }: Props) {
  const router = useRouter();
  const { getToken } = useAuth();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [skill, setSkill] = useState("");
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
    if (!skill.trim()) {
      setFieldError("Skill is required");
      return;
    }

    const token = await getToken();
    if (!token) {
      setApiError("Not authenticated");
      return;
    }

    startTransition(async () => {
      try {
        const zombie = await installZombie(
          workspaceId,
          {
            name: name.trim(),
            description: description.trim() || undefined,
            skill: skill.trim(),
          },
          token,
        );
        router.push(`/zombies/${zombie.id}`);
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
          htmlFor="description"
          className="mb-1 block text-sm font-medium"
        >
          Description <span className="text-muted-foreground">(optional)</span>
        </label>
        <input
          id="description"
          name="description"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="Monitors inbox, scores leads"
          className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
        />
      </div>

      <div>
        <label htmlFor="skill" className="mb-1 block text-sm font-medium">
          Skill
        </label>
        <input
          id="skill"
          name="skill"
          value={skill}
          onChange={(e) => setSkill(e.target.value)}
          placeholder="lead-collector-v1"
          className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
        />
        <p className="mt-1 text-xs text-muted-foreground">
          Skill identifier from your repo{"'"}s{" "}
          <code className="font-mono">samples/</code> folder. A template
          picker ships once the skills catalog endpoint is available.
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
