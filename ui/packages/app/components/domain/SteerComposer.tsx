"use client";

import { ComposerPrimitive } from "@assistant-ui/react";
import { Button, Textarea, cn } from "@agentsfleet/design-system";

const PLACEHOLDER_IDLE = "Steer this agent…";
const PLACEHOLDER_RUNNING = "Agent is working — composer disabled";
const SEND_LABEL = "steer ⏎";

export type SteerComposerProps = {
  /**
   * `true` while a stage is mid-flight (a `received` event without a
   * terminal status yet). When set, the textarea + send button render
   * disabled with a working-state placeholder. Drives the same signal
   * assistant-ui's runtime uses internally to disable Send, mirrored
   * onto the textarea so the user sees the same gate on both controls.
   */
  isRunning: boolean;
};

/**
 * Wraps assistant-ui's `<ComposerPrimitive.*>` with design-system
 * primitives so the visible chrome (textarea + send button) lives in
 * the project token system. The composer's Root is the form: submit
 * fires assistant-ui's onNew, which in `AgentThread` posts via the
 * `steerAgentAction` Server Action and appends an optimistic message.
 */
export function SteerComposer({ isRunning }: SteerComposerProps) {
  const placeholder = isRunning ? PLACEHOLDER_RUNNING : PLACEHOLDER_IDLE;
  return (
    <ComposerPrimitive.Root
      className={cn(
        "border-t border-border bg-card px-xl py-lg",
        "flex flex-col gap-md",
      )}
      aria-label="Steer composer"
    >
      <div
        className={cn(
          "flex flex-col gap-xs rounded-md border border-border bg-background",
          "sm:flex-row sm:items-end sm:gap-md",
          "px-md py-xs",
          "transition-colors duration-snap ease-snap",
          "focus-within:border-pulse",
          isRunning && "bg-muted",
        )}
      >
        <span
          aria-hidden="true"
          className={cn(
            "font-mono text-mono pb-xs",
            "text-muted-foreground",
            !isRunning && "focus-within:text-pulse",
          )}
        >
          ›
        </span>
        <ComposerPrimitive.Input
          asChild
          disabled={isRunning}
          placeholder={placeholder}
        >
          <Textarea
            rows={1}
            className={cn(
              "flex-1 min-h-0 resize-none border-0 bg-transparent px-0 py-md",
              "font-mono text-mono leading-mono text-foreground",
              "placeholder:text-muted-foreground",
              "focus-visible:ring-0 focus-visible:ring-offset-0 focus-visible:border-0",
              isRunning && "text-muted-foreground",
            )}
          />
        </ComposerPrimitive.Input>
        <ComposerPrimitive.Send asChild>
          <Button type="submit" variant="secondary" size="sm">
            {SEND_LABEL}
          </Button>
        </ComposerPrimitive.Send>
      </div>
    </ComposerPrimitive.Root>
  );
}
