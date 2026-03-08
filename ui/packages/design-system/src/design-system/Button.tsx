import { type ComponentProps, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { uiButtonClass, type UiButtonVariant } from "@usezombie/design-system/classes";

type Variant = UiButtonVariant;

type BaseProps = {
  variant?: Variant;
  children: ReactNode;
};

type LinkButtonProps = BaseProps & {
  to: string;
  external?: boolean;
} & Omit<ComponentProps<"a">, "href">;

type NativeButtonProps = BaseProps & ComponentProps<"button">;

export type ButtonProps = LinkButtonProps | NativeButtonProps;

function isLink(props: ButtonProps): props is LinkButtonProps {
  return "to" in props;
}

function classNames(variant: Variant): string {
  return uiButtonClass(variant);
}

export default function Button(props: ButtonProps) {
  const variant = props.variant ?? "primary";
  const cls = classNames(variant);

  if (isLink(props)) {
    const { to, external, variant, children, ...rest } = props;
    void variant;
    if (external || to.startsWith("http") || to.startsWith("mailto:")) {
      return (
        <a className={cls} href={to} target={external ? "_blank" : undefined} rel={external ? "noopener noreferrer" : undefined} {...rest}>
          {children}
        </a>
      );
    }
    return (
      <Link className={cls} to={to} {...(rest as Omit<ComponentProps<typeof Link>, "to" | "className">)}>
        {children}
      </Link>
    );
  }

  const { variant: propsVariant, children, ...rest } = props;
  void propsVariant;
  return (
    <button className={cls} type="button" {...rest}>
      {children}
    </button>
  );
}
