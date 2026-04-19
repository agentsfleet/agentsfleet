import { type ComponentProps } from "react";
import { cn } from "../utils";

type Columns = "two" | "three" | "four";

/*
 * Grid — responsive auto-fit grid. Min column widths tuned per density:
 *   two   = 280px  (cards of decent copy)
 *   three = 240px  (medium tiles)
 *   four  = 220px  (compact tiles)
 */
const columnClass: Record<Columns, string> = {
  two: "grid-cols-[repeat(auto-fit,minmax(280px,1fr))]",
  three: "grid-cols-[repeat(auto-fit,minmax(240px,1fr))]",
  four: "grid-cols-[repeat(auto-fit,minmax(220px,1fr))]",
};

type Props = {
  columns: Columns;
} & ComponentProps<"div">;

export default function Grid({ columns, className, ref, ...rest }: Props) {
  return (
    <div
      ref={ref}
      className={cn("grid gap-lg", columnClass[columns], className)}
      {...rest}
    />
  );
}
