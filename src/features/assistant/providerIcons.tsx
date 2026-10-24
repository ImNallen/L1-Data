import type { ComponentType } from "react";
import type { LimitSource, ProviderKind } from "../../ipc/bindings";
import { cx } from "../../ui/primitives";

type MarkProps = { size?: number; className?: string };

const RAYS = Array.from({ length: 12 }, (_, i) => {
  const angle = ((i * 30 + 8) * Math.PI) / 180;
  const reach = i % 2 === 0 ? 10.5 : 8.6;
  return { x1: 12 + 1.6 * Math.cos(angle), y1: 12 + 1.6 * Math.sin(angle), x2: 12 + reach * Math.cos(angle), y2: 12 + reach * Math.sin(angle) };
});

function ClaudeMark({ size = 16, className }: MarkProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden className={cx("shrink-0 text-claude", className)}>
      <g stroke="currentColor" strokeWidth={2.3} strokeLinecap="round">
        {RAYS.map((r, i) => (
          <line key={i} {...r} />
        ))}
      </g>
    </svg>
  );
}

const PETALS = [
  "M11.217 19.384a3.501 3.501 0 0 0 6.783 -1.217v-5.167l-6 -3.35",
  "M5.214 15.014a3.501 3.501 0 0 0 4.446 5.266l4.34 -2.534v-6.946",
  "M6 7.63c-1.391 -.236 -2.787 .395 -3.534 1.689a3.474 3.474 0 0 0 1.271 4.745l4.263 2.514l6 -3.348",
  "M12.783 4.616a3.501 3.501 0 0 0 -6.783 1.217v5.067l6 3.45",
  "M18.786 8.986a3.501 3.501 0 0 0 -4.446 -5.266l-4.34 2.534v6.946",
  "M18 16.302c1.391 .236 2.787 -.395 3.534 -1.689a3.474 3.474 0 0 0 -1.271 -4.745l-4.308 -2.514l-5.955 3.42",
];

function CodexMark({ size = 16, className }: MarkProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden className={cx("shrink-0 text-codex", className)}>
      <g fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
        {PETALS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
    </svg>
  );
}

export const PROVIDER_ICONS = { Claude: ClaudeMark } satisfies Record<ProviderKind, ComponentType<MarkProps>>;

export const SOURCE_ICONS = { Claude: ClaudeMark, Codex: CodexMark } satisfies Record<LimitSource, ComponentType<MarkProps>>;
