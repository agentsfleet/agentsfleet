"use client";

import { type ReactNode, useState } from "react";
import { cn } from "../utils";
import { Button, type ButtonProps } from "./Button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./Tooltip";

export interface TooltipButtonProps extends ButtonProps {
  tooltip: ReactNode;
}

export function TooltipButton({
  tooltip,
  children,
  className,
  disabled,
  ...rest
}: TooltipButtonProps) {
  const [open, setOpen] = useState(false);
  const button = (
    <Button
      className={cn(disabled ? "pointer-events-none" : null, className)}
      disabled={disabled}
      {...rest}
    >
      {children}
    </Button>
  );

  return (
    <TooltipProvider>
      <Tooltip open={tooltip == null ? false : open} onOpenChange={setOpen}>
        <TooltipTrigger asChild>
          {disabled ? <span className="inline-flex cursor-not-allowed">{button}</span> : button}
        </TooltipTrigger>
        <TooltipContent>{tooltip}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

export default TooltipButton;
