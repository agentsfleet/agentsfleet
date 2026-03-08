import { type ComponentProps, type ReactNode } from "react";

type Props = {
  gap?: boolean;
  children: ReactNode;
} & ComponentProps<"div">;

export default function Section({ gap, children, className, ...rest }: Props) {
  const cls = [gap ? "z-section-gap" : "z-stack", className].filter(Boolean).join(" ");
  return (
    <div className={cls} {...rest}>
      {children}
    </div>
  );
}
