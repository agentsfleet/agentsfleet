import { Loader2Icon } from "lucide-react";

export default function ZombiesLoading() {
  return (
    <div
      role="status"
      aria-live="polite"
      className="flex items-center gap-3 py-16 text-sm text-muted-foreground"
    >
      <Loader2Icon size={18} className="animate-spin" aria-hidden="true" />
      <span>Loading zombies…</span>
    </div>
  );
}
