import { type ComponentProps, type ReactNode } from "react";

type Columns = "two" | "three" | "four";

type Props = {
  columns: Columns;
  children: ReactNode;
} & ComponentProps<"div">;

export default function Grid({ columns, children, className, ...rest }: Props) {
  const cls = ["z-grid", `z-grid--${columns}`, className].filter(Boolean).join(" ");
  return (
    <div className={cls} {...rest}>
      {children}
    </div>
  );
}
