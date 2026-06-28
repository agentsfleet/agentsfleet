"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useFieldArray, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Button,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
} from "@agentsfleet/design-system";
import { XIcon } from "lucide-react";
import { createCredentialAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { CREDENTIAL_NAME_MAX } from "../lib/credential-data";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

type Props = { workspaceId: string };

const STORE_ACTION = "store the credential";
const ADD_FIELD_LABEL = "Add field";
const ADD_SECRET_LABEL = "Add secret";
// Secret names become a vault key; field names become JSON object keys resolved
// at runtime as ${secrets.<name>.<field>}, so both must be reference-safe.
const NAME_PATTERN = /^[A-Za-z0-9_-]+$/;
const FIELD_PATTERN = /^[A-Za-z0-9_]+$/;

const fieldSchema = z.object({
  key: z
    .string()
    .trim()
    .min(1, "Field name is required")
    .regex(FIELD_PATTERN, "Letters, numbers, and underscores only"),
  // Values are stored verbatim (tokens may carry symbols), so they are not
  // trimmed or pattern-checked — only required.
  value: z.string().min(1, "Value is required"),
});

const schema = z.object({
  name: z
    .string()
    .trim()
    .min(1, "Secret name is required")
    .max(CREDENTIAL_NAME_MAX, `Secret name must be ${CREDENTIAL_NAME_MAX} characters or fewer`)
    .regex(NAME_PATTERN, "Letters, numbers, dashes, and underscores only"),
  fields: z
    .array(fieldSchema)
    .min(1, "Add at least one field")
    .superRefine((fields, ctx) => {
      const seen = new Set<string>();
      fields.forEach((entry, index) => {
        const key = entry.key.trim();
        if (key !== "" && seen.has(key)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: [index, "key"],
            message: "Duplicate field name",
          });
        }
        seen.add(key);
      });
    }),
});

type FormValues = z.infer<typeof schema>;

const EMPTY_FIELD = { key: "", value: "" };

export default function AddCredentialForm({ workspaceId }: Props) {
  const router = useRouter();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: "", fields: [{ ...EMPTY_FIELD }] },
  });
  const { fields, append, remove } = useFieldArray({ control: form.control, name: "fields" });

  function onSubmit(values: FormValues) {
    setApiError(null);
    // Field rows → JSON object the vault stores and the runtime resolves by name.
    const data: Record<string, string> = {};
    for (const entry of values.fields) data[entry.key.trim()] = entry.value;

    startTransition(async () => {
      const name = values.name.trim();
      const result = await createCredentialAction(workspaceId, { name, data });
      if (!result.ok) {
        setApiError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: STORE_ACTION,
          }),
        );
        return;
      }
      captureProductEvent(EVENTS.credential_added, { credential_name: name });
      form.reset({ name: "", fields: [{ ...EMPTY_FIELD }] });
      router.refresh();
    });
  }

  return (
    <Form {...form}>
      <form
        onSubmit={(e) => {
          void form.handleSubmit(onSubmit)(e);
        }}
        className="space-y-md"
      >
        <FormField
          control={form.control}
          name="name"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Secret name</FormLabel>
              <FormControl>
                <Input placeholder="stripe" className="font-mono" spellCheck={false} {...field} />
              </FormControl>
              <p className="text-body-sm leading-body-sm text-muted-foreground">
                Use it in a fleet by writing{" "}
                <code className="font-mono">{"${secrets.<name>.<field>}"}</code> in your config.
              </p>
              <FormMessage />
            </FormItem>
          )}
        />

        <div className="space-y-2">
          <span className="block font-mono text-label uppercase leading-label tracking-label text-muted-foreground">
            Fields
          </span>
          <div className="space-y-2">
            {fields.map((row, index) => (
              <div key={row.id} className="flex items-start gap-2">
                <FormField
                  control={form.control}
                  name={`fields.${index}.key`}
                  render={({ field }) => (
                    <FormItem className="w-48 flex-none">
                      <FormControl>
                        <Input
                          aria-label={`Field ${index + 1} name`}
                          placeholder="api_key"
                          className="font-mono"
                          spellCheck={false}
                          autoComplete="off"
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name={`fields.${index}.value`}
                  render={({ field }) => (
                    <FormItem className="flex-1">
                      <FormControl>
                        <Input
                          aria-label={`Field ${index + 1} value`}
                          type="password"
                          placeholder="value (write-only)"
                          className="font-mono"
                          spellCheck={false}
                          autoComplete="off"
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  aria-label={`Remove field ${index + 1}`}
                  disabled={fields.length === 1}
                  onClick={() => remove(index)}
                >
                  <XIcon size={16} />
                </Button>
              </div>
            ))}
          </div>
          <Button
            type="button"
            variant="link"
            size="sm"
            onClick={() => append({ ...EMPTY_FIELD })}
          >
            + {ADD_FIELD_LABEL}
          </Button>
        </div>

        <Button type="submit" disabled={pending} variant="outline">
          {pending ? <Spinner size="sm" srLabel="Adding" /> : null}
          {ADD_SECRET_LABEL}
        </Button>
        {apiError ? <p className="text-body-sm text-destructive">{apiError}</p> : null}
      </form>
    </Form>
  );
}
