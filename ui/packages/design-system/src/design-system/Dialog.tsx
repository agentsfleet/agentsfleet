"use client";

import * as DialogPrimitive from "@radix-ui/react-dialog";
import { type ComponentProps, type HTMLAttributes, useRef } from "react";
import { XIcon } from "@phosphor-icons/react/ssr";
import { cn } from "../utils";
import { Button } from "./Button";

/*
 * Dialog — Radix Dialog composition with semantic utilities. Client
 * boundary because Radix portals + focus trap need DOM APIs. React 19
 * ref-as-prop. Animation utilities (animate-in / fade-in-0 / zoom-in-95)
 * come from tw-animate-css — consumers must `@import "tw-animate-css";`
 * in their root stylesheet (both the app and website do).
 */

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogPortal = DialogPrimitive.Portal;
export const DialogClose = DialogPrimitive.Close;

export type DialogOverlayProps = ComponentProps<typeof DialogPrimitive.Overlay>;

export function DialogOverlay({ className, ref, ...props }: DialogOverlayProps) {
  return (
    <DialogPrimitive.Overlay
      ref={ref}
      className={cn(
        "fixed inset-0 z-50 bg-overlay backdrop-blur-[var(--blur-dialog)] duration-snap ease-snap motion-reduce:animate-none",
        "data-[state=open]:animate-in data-[state=closed]:animate-out",
        "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
        className,
      )}
      {...props}
    />
  );
}

export type DialogContentProps = ComponentProps<typeof DialogPrimitive.Content> & {
  /** Disables the close X while the dialog refuses to close (a send in flight),
   * so it reads as unavailable instead of looking live and doing nothing. */
  closeDisabled?: boolean;
};

export function DialogContent({
  className,
  children,
  closeDisabled = false,
  ref,
  onOpenAutoFocus,
  onCloseAutoFocus,
  ...props
}: DialogContentProps) {
  const openerRef = useRef<Element | null>(null);
  return (
    <DialogPortal>
      <DialogOverlay />
      <DialogPrimitive.Content
        ref={ref}
        className={cn(
          "fixed left-1/2 top-1/2 z-50 -translate-x-1/2 -translate-y-1/2",
          "w-[calc(100%-var(--spacing-3xl))] max-w-dialog rounded-lg border border-border bg-card p-2xl font-sans text-card-foreground shadow-dialog",
          // A fixed, centre-translated panel cannot be scrolled to by the page,
          // so a dialog taller than the viewport puts its own footer out of
          // reach — the submit button is unclickable rather than merely
          // off-screen. Cap the panel at the viewport and scroll inside it.
          "max-h-[calc(100dvh-var(--spacing-3xl))] overflow-y-auto",
          "data-[state=open]:animate-in data-[state=closed]:animate-out",
          "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
          "duration-snap ease-snap motion-reduce:animate-none",
          className,
        )}
        {...props}
        onOpenAutoFocus={(event) => {
          openerRef.current = document.activeElement;
          onOpenAutoFocus?.(event);
        }}
        onCloseAutoFocus={(event) => {
          onCloseAutoFocus?.(event);
          const opener = openerRef.current;
          if (!event.defaultPrevented && opener instanceof HTMLElement && opener !== document.body && opener.isConnected) {
            opener.focus();
            event.preventDefault();
          }
        }}
      >
        {children}
        <DialogCloseButton disabled={closeDisabled} />
      </DialogPrimitive.Content>
    </DialogPortal>
  );
}

function DialogCloseButton({ disabled }: { disabled: boolean }) {
  return (
    <DialogClose asChild>
      <Button variant="ghost" size="icon" className="absolute right-xl top-2xl" disabled={disabled}>
        <XIcon size={16} aria-hidden="true" />
        <span className="sr-only">Close</span>
      </Button>
    </DialogClose>
  );
}

export function DialogHeader({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-col gap-xl pb-xl", className)} {...props} />;
}

export type DialogTitleProps = ComponentProps<typeof DialogPrimitive.Title>;

export function DialogTitle({ className, ref, ...props }: DialogTitleProps) {
  return (
    <DialogPrimitive.Title
      ref={ref}
      className={cn("flex min-h-10 items-center pr-3xl font-sans text-heading font-title leading-heading tracking-display-md", className)}
      {...props}
    />
  );
}

export type DialogDescriptionProps = ComponentProps<typeof DialogPrimitive.Description>;

export function DialogDescription({ className, ref, ...props }: DialogDescriptionProps) {
  return (
    <DialogPrimitive.Description
      ref={ref}
      className={cn("font-sans text-body-sm leading-dialog-description text-muted-foreground", className)}
      {...props}
    />
  );
}

export function DialogFooter({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-wrap justify-end gap-md pt-xl", className)} {...props} />;
}
