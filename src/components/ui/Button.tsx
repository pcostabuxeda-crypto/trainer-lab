import type { ButtonHTMLAttributes } from "react";

type Props = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "ghost" };

export function Button({ variant = "ghost", className = "", ...rest }: Props) {
  const look =
    variant === "primary"
      ? "bg-accent text-accent-ink hover:brightness-110"
      : "border border-line bg-panel-2 text-ink hover:border-muted";
  return (
    <button
      className={`inline-flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium transition disabled:opacity-50 ${look} ${className}`}
      {...rest}
    />
  );
}
