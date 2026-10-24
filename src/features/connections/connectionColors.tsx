import type { Environment } from "../../ipc/bindings";
import { cx } from "../../ui/primitives";

/** `ConnectionConfig.color` stores an `id`. The hues are fixed so they read on light and dark themes alike. */
export const CONNECTION_COLORS = [
  { id: "red", label: "Red", hex: "#ef4444" },
  { id: "amber", label: "Amber", hex: "#f59e0b" },
  { id: "green", label: "Green", hex: "#10b981" },
  { id: "blue", label: "Blue", hex: "#3b82f6" },
  { id: "purple", label: "Purple", hex: "#a855f7" },
] as const satisfies readonly { id: string; label: string; hex: string }[];

export type ConnectionColorId = (typeof CONNECTION_COLORS)[number]["id"];

export function connectionColorId(color: string | null): ConnectionColorId | null {
  return CONNECTION_COLORS.find((c) => c.id === color)?.id ?? null;
}

export function connectionColor(color: string | null): string | null {
  return CONNECTION_COLORS.find((c) => c.id === color)?.hex ?? null;
}

/** Nothing when the connection has no known color. */
export function ConnectionColorDot({ color, className }: { color: string | null; className?: string }) {
  const hex = connectionColor(color);
  if (!hex) return null;
  return <span aria-hidden className={cx("inline-block size-2 shrink-0 rounded-full", className)} style={{ backgroundColor: hex }} />;
}

/** Local and an unset environment draw nothing, so a chip always means a shared server. A 6% fill keeps the label at
 * 4.5:1 in L1 Light. */
const ENVIRONMENT_CHIP: Record<Environment, string | null> = {
  Local: null,
  Staging: "bg-amber/6 text-amber",
  Production: "bg-danger/6 text-danger",
};

export function EnvironmentChip({ environment, testId = "env-chip", className }: { environment: Environment | null; testId?: string; className?: string }) {
  const look = environment && ENVIRONMENT_CHIP[environment];
  if (!look) return null;
  return (
    <span data-testid={testId} className={cx("inline-flex h-[18px] shrink-0 items-center rounded-sm px-1.5 text-ui-11.5 leading-none font-medium", look, className)}>
      {environment}
    </span>
  );
}
