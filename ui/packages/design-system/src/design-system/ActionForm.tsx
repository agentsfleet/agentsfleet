import type { ComponentProps } from "react";
import { cn } from "../utils";

export type ActionFormProps = ComponentProps<"form">;

export function ActionForm({ className, ...props }: ActionFormProps) {
  return <form className={cn("space-y-4", className)} {...props} />;
}
