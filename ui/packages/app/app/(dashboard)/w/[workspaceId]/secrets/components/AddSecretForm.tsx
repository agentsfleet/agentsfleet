"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useFieldArray, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Button,
  cn,
  EYEBROW_CLASS,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
  TooltipButton,
} from "@agentsfleet/design-system";
import { XIcon } from "lucide-react";
import { createSecretAction, replaceSecretAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { SECRET_NAME_MAX } from "../lib/secret-data";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { CREATE_SECRET_TOOLTIP } from "../copy";

type Props = {
  workspaceId: string;
  initialName?: string;
  initialFields?: readonly string[];
  initialHost?: string;
  /** Called after a successful submit — lets a dialog-hosted mount close itself. */
  onDone?: (name: string) => void;
  /** Called when a dialog-hosted mount should close without saving. */
  onCancel?: () => void;
  onPendingChange?: (pending: boolean) => void;
  mode?: "create" | "replace";
};

export const SECRET_FORM_MODE = {CREATE: "create", REPLACE: "replace"} as const;

const STORE_ACTION = "store the secret";
const ADD_FIELD_LABEL = "Add field";
const ADD_SECRET_LABEL = "Create";
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

const HOST_REQUIRED = "Enter the host allowed to receive this access";
const HOST_INVALID = "Enter a host such as api.fly.io, without a URL path or port";
const hostSchema = z.string().trim().min(1, HOST_REQUIRED).refine(value => {
  try {
    const url = new URL(`https://${value}`);
    return url.hostname === value.toLowerCase() && url.port === "" && url.pathname === "/"
      && url.search === "" && url.hash === "" && url.username === "" && url.password === "";
  } catch {return false;}
}, HOST_INVALID);

const schema = z.object({
  host: z.string().optional(),
  name: z
    .string()
    .trim()
    .min(1, "Secret name is required")
    .max(SECRET_NAME_MAX, `Secret name must be ${SECRET_NAME_MAX} characters or fewer`)
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

export default function AddSecretForm(props: Props) {
  const state = useSecretForm(props);
  const {form, pending, apiError, onSubmit} = state;
  const submitLabel = props.mode === SECRET_FORM_MODE.REPLACE ? "Save changes" : ADD_SECRET_LABEL;
  return <Form {...form}>
    <form onSubmit={event => {void form.handleSubmit(onSubmit)(event);}} className="space-y-md">
      <SecretNameField form={form} readOnly={props.mode === SECRET_FORM_MODE.REPLACE} />
      {props.initialHost !== undefined ? <SecretHostField form={form} /> : null}
      <SecretFields form={form} />
      <div className="flex justify-end gap-2">
        {props.onCancel ? <Button type="button" variant="ghost" disabled={pending} onClick={props.onCancel}>Cancel</Button> : null}
        <TooltipButton type="submit" disabled={pending} aria-busy={pending || undefined} aria-label={submitLabel}
          tooltip={props.mode === SECRET_FORM_MODE.REPLACE ? "Replace the fields stored under this name" : CREATE_SECRET_TOOLTIP}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}{submitLabel}
        </TooltipButton>
      </div>
      {apiError ? <p className="text-body-sm text-destructive">{apiError}</p> : null}
    </form>
  </Form>;
}

function useSecretForm({workspaceId, initialName = "", initialFields = [], initialHost, onDone, onPendingChange, mode = SECRET_FORM_MODE.CREATE}: Props) {
  const router = useRouter();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({
    resolver: zodResolver(initialHost === undefined ? schema : schema.extend({host: hostSchema})),
    defaultValues: {name: initialName, host: initialHost,
      fields: initialFields.length ? initialFields.map(key => ({key, value: ""})) : [{...EMPTY_FIELD}]},
  });
  const onSubmit = (values: FormValues): void => {
    setApiError(null);
    const data: Record<string, string> = Object.fromEntries(values.fields.map(entry => [entry.key.trim(), entry.value]));
    if (values.host) data.host = values.host;
    onPendingChange?.(true);
    startTransition(async () => {
      try {
        const name = mode === SECRET_FORM_MODE.REPLACE ? initialName : values.name.trim();
        const result = mode === SECRET_FORM_MODE.REPLACE
          ? await replaceSecretAction(workspaceId, name, data) : await createSecretAction(workspaceId, {name, data});
        if (!result.ok) {
          setApiError(presentErrorString({errorCode: result.errorCode, message: result.error, action: STORE_ACTION}));
          return;
        }
        if (mode === SECRET_FORM_MODE.CREATE) captureProductEvent(EVENTS.secret_added, {secret_name: name});
        form.reset({name: "", host: initialHost, fields: [{...EMPTY_FIELD}]});
        onDone?.(name);
        router.refresh();
      } catch {
        setApiError("The secret save could not be confirmed. Check Manage secrets before trying again.");
      } finally {onPendingChange?.(false);}
    });
  };
  return {form, pending, apiError, onSubmit};
}

type SecretForm = ReturnType<typeof useSecretForm>["form"];

function SecretNameField({form, readOnly}: {form: SecretForm; readOnly: boolean}) {
  return <FormField control={form.control} name="name" render={({field}) => <FormItem>
    <FormLabel>Secret name</FormLabel>
    <FormControl><Input placeholder="stripe" className="font-mono" spellCheck={false} readOnly={readOnly} {...field} /></FormControl>
    <p className="text-body-sm leading-body-sm text-muted-foreground">
      Reference its fields in instructions as <code className="font-mono">${"{secrets.name.field}"}</code>.
    </p>
    <FormMessage />
  </FormItem>} />;
}

function SecretHostField({form}: {form: SecretForm}) {
  return <FormField control={form.control} name="host" render={({field}) => <FormItem>
    <FormLabel>Allowed host</FormLabel>
    <FormControl><Input placeholder="api.fly.io" autoComplete="off" spellCheck={false} {...field} /></FormControl>
    <p className="text-body-sm text-muted-foreground">This access can only be sent to this host. Saving does not verify the token.</p>
    <FormMessage />
  </FormItem>} />;
}

function SecretFields({form}: {form: SecretForm}) {
  const {fields, append, remove} = useFieldArray({control: form.control, name: "fields"});
  return <div className="space-y-2">
    <span className={cn(EYEBROW_CLASS, "block text-muted-foreground")}>Fields</span>
    <div className="space-y-2">{fields.map((row, index) =>
      <SecretFieldRow key={row.id} form={form} index={index} canRemove={fields.length > 1} onRemove={() => remove(index)} />
    )}</div>
    <Button type="button" variant="link" size="sm" onClick={() => append({...EMPTY_FIELD})}>+ {ADD_FIELD_LABEL}</Button>
  </div>;
}

function SecretFieldRow({form, index, canRemove, onRemove}: {form: SecretForm; index: number; canRemove: boolean; onRemove: () => void}) {
  return <div className="flex items-start gap-2">
    <FormField control={form.control} name={`fields.${index}.key`} render={({field}) => <FormItem className="w-48 flex-none">
      <FormControl><Input aria-label={`Field ${index + 1} name`} placeholder="api_key" className="font-mono" spellCheck={false} autoComplete="off" {...field} /></FormControl>
      <FormMessage />
    </FormItem>} />
    <FormField control={form.control} name={`fields.${index}.value`} render={({field}) => <FormItem className="flex-1">
      <FormControl><Input aria-label={`Field ${index + 1} value`} type="password" placeholder="value (write-only)" className="font-mono" spellCheck={false} autoComplete="off" {...field} /></FormControl>
      <FormMessage />
    </FormItem>} />
    <Button type="button" variant="ghost" size="sm" aria-label={`Remove field ${index + 1}`} disabled={!canRemove} onClick={onRemove}>
      <XIcon size={16} />
    </Button>
  </div>;
}
