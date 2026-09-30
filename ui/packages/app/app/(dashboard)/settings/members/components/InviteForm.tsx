"use client";

import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  ActionForm,
  Button,
  CopyButton,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Time,
} from "@agentsfleet/design-system";
import type { InviteSummary } from "@/lib/api/invites";
import { presentErrorString } from "@/lib/errors";
import { createInviteAction } from "../actions";

// The backend lowercases and checks the address itself; this only spares a
// round trip for something that is plainly not one.
const schema = z.object({
  email: z.string().trim().pipe(z.email("Enter an email address")),
});

type FormValues = z.infer<typeof schema>;

export function InviteForm({ onCreated }: { onCreated: () => void }) {
  const [created, setCreated] = useState<InviteSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: { email: "" } });

  function onSubmit(values: FormValues) {
    setError(null);
    startTransition(async () => {
      const result = await createInviteAction(values.email);
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "send the invite" }));
        return;
      }
      setCreated(result.data);
      form.reset();
      onCreated();
    });
  }

  return (
    <div className="space-y-4">
      <Form {...form}>
        <ActionForm
          onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }}
          className="flex flex-col gap-sm space-y-0 sm:flex-row sm:items-end"
          aria-busy={pending}
          // The schema's message, not the browser's bubble, explains a bad address.
          noValidate
        >
          <FormField
            control={form.control}
            name="email"
            render={({ field }) => (
              <FormItem className="min-w-0 flex-1">
                <FormLabel>Email</FormLabel>
                <FormControl>
                  <Input type="email" placeholder="teammate@example.com" autoComplete="off" {...field} />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          <Button type="submit" disabled={pending}>Invite</Button>
        </ActionForm>
      </Form>
      {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
      {created ? <InviteReady invite={created} /> : null}
    </div>
  );
}

// The link is what the invitee opens, so it shows the moment the invite
// exists, with the copy action beside it.
function InviteReady({ invite }: { invite: InviteSummary }) {
  return (
    <div className="space-y-2 rounded-md border border-border p-md" data-testid="invite-ready">
      <p className="text-sm">
        Invite ready for {invite.email}. Send them this link; it expires{" "}
        <Time value={new Date(invite.expires_at)} format="relative" />.
      </p>
      <div className="flex min-w-0 items-center gap-sm">
        <code className="min-w-0 flex-1 truncate text-label text-muted-foreground">{invite.link}</code>
        <CopyButton value={invite.link} label="Copy invite link" showLabel />
      </div>
    </div>
  );
}
