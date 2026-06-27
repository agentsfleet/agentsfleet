"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
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
  Spinner,
  Textarea,
} from "@agentsfleet/design-system";
import { createCredentialAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { CREDENTIAL_NAME_MAX, parseCredentialDataObject } from "../lib/credential-data";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

type Props = { workspaceId: string };

// Re-exported from the shared credential-data parser so existing importers
// (and tests) keep their import site; the implementation lives in one place.
export { jsonParseErrorMessage } from "../lib/credential-data";

const schema = z.object({
  name: z
    .string()
    .trim()
    .min(1, "Credential name is required")
    .max(CREDENTIAL_NAME_MAX, `Credential name must be ${CREDENTIAL_NAME_MAX} characters or fewer`),
  data_json: z
    .string()
    .trim()
    .min(1, "Credential data is required")
    .superRefine((s, ctx) => {
      // Empty is already gated by .min(1) above; share the parse/shape rules.
      const result = parseCredentialDataObject(s, "Credential data is required");
      if (!result.ok) ctx.addIssue({ code: z.ZodIssueCode.custom, message: result.message });
    }),
});
type FormValues = z.infer<typeof schema>;

export default function AddCredentialForm({ workspaceId }: Props) {
  const router = useRouter();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: "", data_json: "" },
  });

  function onSubmit(values: FormValues) {
    setApiError(null);
    startTransition(async () => {
      // zod's superRefine on `data_json` (see schema above) runs the same
      // JSON.parse + object-shape checks before onSubmit fires, so by the
      // time we land here `values.data_json` is guaranteed parseable.
      // No defensive try/catch — the framework already proved it.
      const data = JSON.parse(values.data_json) as Record<string, unknown>;
      const result = await createCredentialAction(workspaceId, { name: values.name.trim(), data });
      if (!result.ok) {
        setApiError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "store the credential",
          }),
        );
        return;
      }
      captureProductEvent(EVENTS.credential_added, { credential_name: values.name.trim() });
      form.reset({ name: "", data_json: "" });
      router.refresh();
    });
  }

  return (
    <Form {...form}>
      <form
        onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }}
        className="grid gap-3 lg:grid-cols-3 lg:items-start"
      >
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
                Use as <code>{"${secrets.<name>.<field>}"}</code>.
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
                  rows={4}
                  spellCheck={false}
                  autoComplete="off"
                  placeholder='{"host": "api.machines.dev", "api_token": "FLY_API_TOKEN"}'
                  className="font-mono text-sm"
                  {...field}
                />
              </FormControl>
              <FormDescription>
                Write-only after save.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
        <Button type="submit" disabled={pending} variant="outline" className="lg:mt-xl">
          {pending ? <Spinner size="sm" srLabel="Adding" /> : null}
          Add secret
        </Button>
        {apiError ? (
          <p className="text-body-sm text-destructive lg:col-span-3">{apiError}</p>
        ) : null}
      </form>
    </Form>
  );
}
