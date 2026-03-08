import { type ComponentProps, type ReactNode } from "react";
import { uiCardClass } from "@usezombie/design-system/classes";

type Props = {
  featured?: boolean;
  children: ReactNode;
} & ComponentProps<"article">;

export default function Card({ featured, children, className, ...rest }: Props) {
  const cls = uiCardClass(featured, className);
  return (
    <article className={cls} {...rest}>
      {children}
    </article>
  );
}
