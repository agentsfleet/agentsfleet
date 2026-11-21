import { useId, type ReactNode } from "react";
import { Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@agentsfleet/design-system";

type Props = {
  label: string;
  value: string;
  choices: {value: string; label: ReactNode}[];
  disabled: boolean;
  onChange: (value: string) => void;
  onOpenChange?: (open: boolean) => void;
};

export function BillingFilter({label, value, choices, disabled, onChange, onOpenChange}: Props) {
  const id = useId();
  return (
    <div className="billing-filter">
      <Label htmlFor={id} appearance="filter">{label}</Label>
      <Select value={value} disabled={disabled} onValueChange={onChange} onOpenChange={onOpenChange}>
        <SelectTrigger id={id} aria-label={label} appearance="filter"><SelectValue /></SelectTrigger>
        <SelectContent>
          {choices.map(choice => <SelectItem key={choice.value} value={choice.value}>{choice.label}</SelectItem>)}
        </SelectContent>
      </Select>
    </div>
  );
}
