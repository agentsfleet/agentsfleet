"use client";

import * as DialogPrimitive from "@radix-ui/react-dialog";
import { type ComponentProps, type HTMLAttributes, useEffect, useRef } from "react";
import { XIcon } from "@phosphor-icons/react/ssr";
import { cn } from "../utils";
import { Button } from "./Button";
import { focusDialogField } from "./dialog-focus";

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
  /** Wider forms keep their actions visible while the phone body scrolls. */
  size?: "default" | "wide";
};

const DIALOG_CONTENT_CLASS = [
  "[--control-focus-surface:var(--surface-1)]",
  "fixed left-1/2 top-1/2 z-50 -translate-x-1/2 -translate-y-1/2",
  "w-[calc(100%-var(--spacing-3xl))] max-w-dialog rounded-lg border border-border bg-card p-2xl font-sans text-card-foreground shadow-dialog",
  "max-h-[calc(100dvh-var(--spacing-3xl))] overflow-y-auto",
  "data-[state=open]:animate-in data-[state=closed]:animate-out",
  "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
  "duration-snap ease-snap motion-reduce:animate-none",
].join(" ");

const WIDE_DIALOG_CLASS = [
  "max-w-model-dialog [&_[data-slot=dialog-footer]_button]:min-h-11",
  "max-[42.5rem]:p-5 max-[42.5rem]:[&_[data-dialog-close]]:top-5",
  "max-[42.5rem]:[&_[data-slot=dialog-footer]]:sticky max-[42.5rem]:[&_[data-slot=dialog-footer]]:-bottom-5",
  "max-[42.5rem]:[&_[data-slot=dialog-footer]]:z-10 max-[42.5rem]:[&_[data-slot=dialog-footer]]:border-t",
  "max-[42.5rem]:[&_[data-slot=dialog-footer]]:border-border max-[42.5rem]:[&_[data-slot=dialog-footer]]:bg-card",
  "max-[42.5rem]:[&_[data-slot=dialog-footer]]:-mx-5 max-[42.5rem]:[&_[data-slot=dialog-footer]]:-mb-5",
  "max-[42.5rem]:[&_[data-slot=dialog-footer]]:px-5 max-[42.5rem]:[&_[data-slot=dialog-footer]]:py-xl",
].join(" ");

export function DialogContent({
  className,
  children,
  closeDisabled = false,
  size = "default",
  ref,
  onOpenAutoFocus,
  onCloseAutoFocus,
  ...props
}: DialogContentProps) {
  const openerRef = useRef<Element | null>(null);
  const focusCleanup = useRef<(() => void) | null>(null);
  useEffect(() => () => focusCleanup.current?.(), []);
  return (
    <DialogPortal>
      <DialogOverlay />
      <DialogPrimitive.Content
        ref={ref}
        className={cn(DIALOG_CONTENT_CLASS, size === "wide" && WIDE_DIALOG_CLASS, className)}
        {...props}
        onOpenAutoFocus={(event) => {
          openerRef.current = document.activeElement;
          onOpenAutoFocus?.(event);
          focusCleanup.current?.();
          focusCleanup.current = event.defaultPrevented ? null : focusDialogField(event.target as HTMLElement, event);
        }}
        onCloseAutoFocus={(event) => {
          focusCleanup.current?.();
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
      <Button data-dialog-close="" variant="ghost" size="icon" className="absolute right-xl top-2xl size-10 min-h-10 pointer-coarse:min-h-10 pointer-coarse:min-w-10" disabled={disabled}>
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

export function DialogTitle({ className, children, ref, ...props }: DialogTitleProps) {
  return (
    <DialogPrimitive.Title
      ref={ref}
      className={cn("flex min-h-10 items-center pr-3xl font-sans text-heading font-title leading-heading tracking-display-md", className)}
      {...props}
    >
      <span className="min-w-0 truncate">{children}</span>
    </DialogPrimitive.Title>
  );
}

export type DialogDescriptionProps = ComponentProps<typeof DialogPrimitive.Description>;

export function DialogDescription({ className, ref, ...props }: DialogDescriptionProps) {
  return (
    <DialogPrimitive.Description
      ref={ref}
      className={cn("break-words font-sans text-body-sm leading-dialog-description text-muted-foreground", className)}
      {...props}
    />
  );
}

export function DialogFooter({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div data-slot="dialog-footer" className={cn("flex flex-nowrap justify-end gap-md pt-2xl", className)} {...props} />;
}
