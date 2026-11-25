"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useFieldArray, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Button,
  DialogFooter,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
} from "@agentsfleet/design-system";
import { XIcon, CaretDownIcon } from "@phosphor-icons/react/ssr";
import { createSecretAction, replaceSecretAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

type Props = {
  showValueNotice?: boolean;
  workspaceId: string;
  initialName?: string;
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
const ADD_SECRET_LABEL = "Add";
// New names support instruction references; existing vault names stay immutable.
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

function formSchema(mode: Props["mode"]) {
  return mode === SECRET_FORM_MODE.REPLACE ? schema.extend({name: z.string()}) : schema;
}

const EMPTY_FIELD = { key: "", value: "" };

export default function AddSecretForm(props: Props) {
  const state = useSecretForm(props);
  const {form, pending, apiError, onSubmit} = state;
  const [namedFields, setNamedFields] = useState(props.mode === SECRET_FORM_MODE.REPLACE);
  const submitLabel = props.mode === SECRET_FORM_MODE.REPLACE ? "Replace value" : ADD_SECRET_LABEL;
  return <Form {...form}>
    <form onSubmit={event => {void form.handleSubmit(onSubmit)(event);}}>
      <div className="space-y-xl">
        <SecretNameField form={form} readOnly={props.mode === SECRET_FORM_MODE.REPLACE} />
        {namedFields ? <SecretFields form={form} /> : <>
          <FormField control={form.control} name="fields.0.value" render={({field}) => <FormItem>
            <FormLabel>Value</FormLabel><FormControl><Input type="password" autoComplete="off" spellCheck={false} {...field} /></FormControl><FormMessage />
          </FormItem>} />
          <Button type="button" variant="ghost" className="px-0 text-muted-foreground" onClick={() => {form.setValue("fields.0.key", ""); setNamedFields(true);}}>Use named fields<CaretDownIcon size={16} aria-hidden="true" /></Button>
        </>}
        {props.showValueNotice !== false ? <p className="text-eyebrow text-text-dim">Secret values are hidden after saving.</p> : null}
        {apiError ? <p className="text-body-sm text-destructive">{apiError}</p> : null}
      </div>
      <DialogFooter>
        {props.onCancel ? <Button type="button" variant="outline" disabled={pending} onClick={props.onCancel}>Cancel</Button> : null}
        <Button type="submit" disabled={pending} aria-busy={pending || undefined} aria-label={submitLabel}
          >
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}{submitLabel}
        </Button>
      </DialogFooter>
    </form>
  </Form>;
}

function useSecretForm({workspaceId, initialName = "", onDone, onPendingChange, mode = SECRET_FORM_MODE.CREATE}: Props) {
  const router = useRouter();
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({
    resolver: zodResolver(formSchema(mode)),
    defaultValues: {name: initialName, fields: [{key: mode === SECRET_FORM_MODE.CREATE ? "value" : "", value: ""}]},
  });
  const onSubmit = (values: FormValues): void => {
    setApiError(null);
    const data: Record<string, string> = Object.fromEntries(values.fields.map(entry => [entry.key.trim(), entry.value]));
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
        form.reset({name: "", fields: [{key: mode === SECRET_FORM_MODE.CREATE ? "value" : "", value: ""}]});
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
    <FormLabel>Name</FormLabel>
    <FormControl><Input placeholder="stripe" className="font-mono" spellCheck={false} readOnly={readOnly} {...field} /></FormControl>
    <p className="text-eyebrow leading-eyebrow text-text-dim">
      Reference its fields in instructions as <code className="font-mono">${"{secrets.name.field}"}</code>.
    </p>
    <FormMessage />
  </FormItem>} />;
}

function SecretFields({form}: {form: SecretForm}) {
  const {fields, append, remove} = useFieldArray({control: form.control, name: "fields"});
  return <div className="space-y-2">
    <span className="block text-label font-medium">Named fields</span>
    <div className="space-y-lg">{fields.map((row, index) =>
      <SecretFieldRow key={row.id} form={form} index={index} canRemove={fields.length > 1} onRemove={() => remove(index)} />
    )}</div>
    <Button type="button" variant="link" size="sm" onClick={() => append({...EMPTY_FIELD})}>+ {ADD_FIELD_LABEL}</Button>
  </div>;
}

function SecretFieldRow({form, index, canRemove, onRemove}: {form: SecretForm; index: number; canRemove: boolean; onRemove: () => void}) {
  return <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-lg sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
    <FormField control={form.control} name={`fields.${index}.key`} render={({field}) => <FormItem className="min-w-0">
      <FormControl><Input aria-label={`Field ${index + 1} name`} placeholder="api_key" className="font-mono" spellCheck={false} autoComplete="off" {...field} /></FormControl>
      <FormMessage />
    </FormItem>} />
    <FormField control={form.control} name={`fields.${index}.value`} render={({field}) => <FormItem className="col-span-full col-start-1 row-start-2 min-w-0 sm:col-span-1 sm:col-start-2 sm:row-start-1">
      <FormControl><Input aria-label={`Field ${index + 1} value`} type="password" placeholder="value (write-only)" className="font-mono" spellCheck={false} autoComplete="off" {...field} /></FormControl>
      <FormMessage />
    </FormItem>} />
    <Button type="button" variant="ghost" size="sm" className="col-start-2 row-start-1 sm:col-start-3" aria-label={`Remove field ${index + 1}`} disabled={!canRemove} onClick={onRemove}>
      <XIcon size={16} />
    </Button>
  </div>;
}
