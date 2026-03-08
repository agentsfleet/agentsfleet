import Button from "./Button";
import Terminal from "./Terminal";

type Action = {
  label: string;
  to: string;
  variant?: "primary" | "ghost" | "double-border";
};

type Props = {
  title: string;
  command: string;
  actions: Action[];
};

export default function InstallBlock({ title, command, actions }: Props) {
  return (
    <div className="z-install-block">
      <h2>{title}</h2>
      <Terminal label={`${title} command`} copyable>{command}</Terminal>
      <div className="z-btn-row">
        {actions.map((a) => (
          <Button key={a.label} to={a.to} variant={a.variant ?? "primary"}>
            {a.label}
          </Button>
        ))}
      </div>
    </div>
  );
}
