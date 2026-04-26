"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { Loader2Icon } from "lucide-react";
import {
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
import { useClientToken } from "@/lib/auth/client";
import { createCredential } from "@/lib/api/credentials";

type Props = { workspaceId: string };

const schema = z.object({
  name: z
    .string()
    .trim()
    .min(1, "Credential name is required")
    .max(64, "Credential name must be 64 characters or fewer"),
  data_json: z
    .string()
    .trim()
    .min(1, "Credential data is required")
    .superRefine((s, ctx) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(s);
      } catch (err) {
        const message = err instanceof Error ? err.message : "Invalid JSON";
        ctx.addIssue({ code: z.ZodIssueCode.custom, message: `Invalid JSON: ${message}` });
        return;
      }
      if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: "Data must be a JSON object — strings, arrays, and scalars are rejected",
        });
        return;
      }
      if (Object.keys(parsed as Record<string, unknown>).length === 0) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: "Object must have at least one field",
        });
      }
    }),
});
type FormValues = z.infer<typeof schema>;

export default function AddCredentialForm({ workspaceId }: Props) {
  const router = useRouter();
  const { getToken } = useClientToken();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: "", data_json: "" },
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
        const data = JSON.parse(values.data_json) as Record<string, unknown>;
        await createCredential(workspaceId, { name: values.name.trim(), data }, token);
        form.reset({ name: "", data_json: "" });
        router.refresh();
      } catch (e) {
        const err = e as Error & { status?: number };
        setApiError(err.message || "Failed to store credential");
      }
    });
  }

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
        <FormField
          control={form.control}
          name="name"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Name</FormLabel>
              <FormControl>
                <Input placeholder="fly" {...field} />
              </FormControl>
              <FormDescription>
                Referenced from a zombie&apos;s <code>credentials:</code> list and substituted as
                {" "}
                <code>{"${secrets.<name>.<field>}"}</code> at the tool bridge.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
        <FormField
          control={form.control}
          name="data_json"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Data (JSON object)</FormLabel>
              <FormControl>
                <Textarea
                  rows={8}
                  spellCheck={false}
                  autoComplete="off"
                  placeholder='{"host": "api.machines.dev", "api_token": "FLY_API_TOKEN"}'
                  className="font-mono text-sm"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                Plaintext is the canonical-stringified form of this object,
                envelope-encrypted at rest. Values are never returned by the API.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
        {apiError && <p className="text-sm text-red-600">{apiError}</p>}
        <Button type="submit" disabled={pending}>
          {pending && <Loader2Icon size={14} className="mr-2 animate-spin" />}
          Store credential
        </Button>
      </form>
    </Form>
  );
}
