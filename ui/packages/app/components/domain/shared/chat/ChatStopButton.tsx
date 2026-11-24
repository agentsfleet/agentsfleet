import { Button } from "@agentsfleet/design-system";
import { SquareIcon } from "@phosphor-icons/react/ssr";

export function ChatStopButton({label, disabled, onClick}: {label: string; disabled: boolean; onClick: () => void}) {
  return <Button type="button" variant="secondary" size="icon-row" aria-label={label} disabled={disabled} onClick={onClick}
    className="shrink-0 self-end rounded-full border-destructive/35 bg-destructive/10 text-destructive hover:border-destructive/50 hover:bg-destructive/20 hover:text-destructive [&_svg]:size-4.5">
    <SquareIcon size={18} weight="fill" aria-hidden="true" />
  </Button>;
}
