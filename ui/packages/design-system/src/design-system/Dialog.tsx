"use client";

import * as DialogPrimitive from "@radix-ui/react-dialog";
import { type ComponentProps, type HTMLAttributes } from "react";
import { cn } from "../utils";

/*
 * Dialog — Radix Dialog composition with semantic utilities. Client
 * boundary because Radix portals + focus trap need DOM APIs. React 19
 * ref-as-prop. Animation utilities (animate-in / fade-in-0 / zoom-in-95)
 * come from tw-animate-css — consumers must `@import "tw-animate-css";`
 * in their root stylesheet (both the app and website do).
 *
 * The close-button X icon is inlined as an SVG so the design-system
 * package doesn't take on `lucide-react` (or any icon library) as a
 * transitive dependency.
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
        "fixed inset-0 z-50 bg-black/60 backdrop-blur-sm",
        "data-[state=open]:animate-in data-[state=closed]:animate-out",
        "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
        className,
      )}
      {...props}
    />
  );
}

export type DialogContentProps = ComponentProps<typeof DialogPrimitive.Content>;

export function DialogContent({ className, children, ref, ...props }: DialogContentProps) {
  return (
    <DialogPortal>
      <DialogOverlay />
      <DialogPrimitive.Content
        ref={ref}
        className={cn(
          "fixed left-1/2 top-1/2 z-50 -translate-x-1/2 -translate-y-1/2",
          "w-full max-w-lg rounded-xl border border-border bg-card p-6 shadow-xl",
          "data-[state=open]:animate-in data-[state=closed]:animate-out",
          "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
          "data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95",
          "data-[state=closed]:slide-out-to-top-[2%] data-[state=open]:slide-in-from-top-[2%]",
          className,
        )}
        {...props}
      >
        {children}
        <DialogClose
          className={cn(
            "absolute right-4 top-4 rounded-sm opacity-70 transition-opacity",
            "hover:opacity-100 focus:outline-none focus:ring-1 focus:ring-ring",
          )}
        >
          <svg
            className="h-4 w-4"
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
          <span className="sr-only">Close</span>
        </DialogClose>
      </DialogPrimitive.Content>
    </DialogPortal>
  );
}

export function DialogHeader({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-col gap-1.5 pb-4", className)} {...props} />;
}

export type DialogTitleProps = ComponentProps<typeof DialogPrimitive.Title>;

export function DialogTitle({ className, ref, ...props }: DialogTitleProps) {
  return (
    <DialogPrimitive.Title
      ref={ref}
      className={cn("text-base font-semibold leading-none tracking-tight", className)}
      {...props}
    />
  );
}

export type DialogDescriptionProps = ComponentProps<typeof DialogPrimitive.Description>;

export function DialogDescription({ className, ref, ...props }: DialogDescriptionProps) {
  return (
    <DialogPrimitive.Description
      ref={ref}
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    />
  );
}

export function DialogFooter({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex justify-end gap-2 pt-4", className)} {...props} />;
}
