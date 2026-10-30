import { ChevronsUpDownIcon, Undo2Icon } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { Density, FontSetting, ReduceMotion } from "../../ipc/bindings";
import { gridRowHeight, useReducedMotion } from "../../shell/appearance";
import { DEFAULT_SETTINGS, useWorkspace } from "../../state/workspace";
import { canvasFontProbe, FONT_SIZES, fontChoices, fontStack, type FontKind } from "../../themes/fonts";
import { cx, IconButton, DatabaseBadge } from "../../ui/primitives";
import { saveSettings } from "../editor/runActions";
import { chooseDensity, chooseFont, chooseReduceMotion, previewPanelMotion, savePanelMotion } from "./appearanceActions";
import { Row } from "./SettingLayout";
import { settingLabel, type SettingId } from "./settingsIndex";

const DENSITIES: Density[] = ["Compact", "Standard", "Spacious"];
const REDUCE_MOTION: ReduceMotion[] = ["System", "On", "Off"];

/** A row of native radios drawn as one segmented control, each choice labelled with its own value. */
function Segmented<T extends string>({
  setting,
  choices,
  value,
  onChoose,
}: {
  setting: SettingId;
  choices: readonly T[];
  value: T;
  onChoose: (choice: T) => void;
}) {
  return (
    <div role="radiogroup" aria-label={settingLabel(setting)} data-testid={`setting-${setting}`} className="flex rounded-lg border border-control p-0.5">
      {choices.map((c) => (
        <label
          key={c}
          data-testid={`setting-${setting}-${c}`}
          className="cursor-pointer rounded-md px-2.5 py-1 text-ui-12.5 text-muted hover:text-fg has-checked:bg-selected has-checked:text-fg has-focus-visible:outline has-focus-visible:outline-focus"
        >
          <input type="radio" name={setting} value={c} checked={c === value} onChange={() => onChoose(c)} className="sr-only" />
          {c}
        </label>
      ))}
    </div>
  );
}

export function Select({
  testId,
  label,
  value,
  options,
  onChange,
  className,
  disabled,
}: {
  testId: string;
  label: string;
  value: string;
  options: readonly { value: string; label: string }[];
  onChange: (value: string) => void;
  className?: string;
  disabled?: boolean;
}) {
  return (
    <div className={cx("relative", className)}>
      <select
        data-testid={testId}
        aria-label={label}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        className="h-8 w-full cursor-pointer appearance-none rounded-lg border border-control bg-bg pr-7 pl-2.5 text-ui-13 text-fg outline-none hover:bg-hover focus-visible:border-focus disabled:cursor-default disabled:opacity-40 disabled:hover:bg-bg"
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <ChevronsUpDownIcon size={14} className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-dim" />
    </div>
  );
}

const noFonts = () => false;

const FONT_SETTINGS = { interface: "interfaceFont", code: "codeFont" } as const satisfies Record<FontKind, SettingId>;

function FontControls({ kind, font, isInstalled }: { kind: FontKind; font: FontSetting; isInstalled: (probe: string) => boolean }) {
  const name = settingLabel(FONT_SETTINGS[kind]);
  const families = fontChoices(kind, font.family, isInstalled).map((f) => ({ value: f, label: f }));
  const sizes = FONT_SIZES[kind].map((n) => ({ value: String(n), label: `${n} px` }));
  const testId = `setting-${FONT_SETTINGS[kind]}`;
  return (
    <>
      <Select
        testId={`${testId}-family`}
        label={`${name} family`}
        value={font.family}
        options={families}
        onChange={(family) => void chooseFont(kind, { family })}
        className="w-44"
      />
      <Select
        testId={`${testId}-size`}
        label={`${name} size`}
        value={String(font.size)}
        options={sizes}
        onChange={(size) => void chooseFont(kind, { size: Number(size) })}
        className="w-[84px]"
      />
    </>
  );
}

function InterfaceSample({ font }: { font: FontSetting }) {
  return (
    <p
      className="mt-3 rounded-lg border border-control bg-bg px-3 py-2.5 leading-relaxed text-muted"
      style={{ fontFamily: fontStack("interface", font.family), fontSize: font.size }}
    >
      Open{" "}
      <span className="inline-flex items-center gap-1 rounded-md bg-pill px-1.5 align-middle text-fg">
        <DatabaseBadge databaseKind="Postgres" /> shop
      </span>{" "}
      and browse <span className="rounded-md bg-pill px-1.5 text-fg">public.orders</span>, then sort by total and filter to the paid ones.
    </p>
  );
}

const SAMPLE_ROWS = [
  ["1001", "Ada Lovelace", "129.50", "paid"],
  ["1002", "Grace Hopper", "42.00", "pending"],
  ["1003", "Edsger Dijkstra", "7,310.25", null],
] as const;
const SAMPLE_COLUMNS = ["id", "customer", "total", "status"];
const NUMERIC = new Set([0, 2]);

function CodeSample({ font, density }: { font: FontSetting; density: Density }) {
  const rowH = gridRowHeight(font.size, density);
  const kw = (t: string) => <span className="text-keyword">{t}</span>;
  return (
    <div
      className="mt-3 overflow-hidden rounded-lg border border-control bg-bg"
      style={{ fontFamily: fontStack("code", font.family), fontSize: font.size, lineHeight: `${Math.round(font.size * 1.54)}px` }}
    >
      <pre className="px-3 py-2.5 text-(color:--code)">
        {kw("select")} id, customer, total, status{"\n"}
        {kw("from")} orders {kw("where")} status &lt;&gt; <span className="text-green">'refunded'</span>
        {"\n"}
        {kw("order by")} total {kw("desc")} {kw("limit")} <span className="text-amber">3</span>;
      </pre>
      <table className="w-full border-t border-grid bg-bg tabular-nums" style={{ lineHeight: 1 }}>
        <thead>
          <tr style={{ height: rowH }} className="border-b border-grid font-sans text-ui-12.5 text-muted">
            {SAMPLE_COLUMNS.map((c, i) => (
              <th key={c} className={cx("border-r border-grid px-2 font-medium last:border-r-0", NUMERIC.has(i) ? "text-right" : "text-left")}>
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {SAMPLE_ROWS.map((row) => (
            <tr key={row[0]} style={{ height: rowH }} className="border-b border-grid text-fg last:border-b-0">
              {row.map((v, i) => (
                <td key={i} className={cx("border-r border-grid px-2 last:border-r-0", NUMERIC.has(i) && "text-right")}>
                  {v ?? <span className="text-dim">NULL</span>}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const PREVIEW_HOLD_MS = 400;

/** Plays what the app does: a slide, or under reduced motion a panel that snaps open and shut. */
function MotionPreview({ ms, reduced }: { ms: number; reduced: boolean }) {
  const panel = useRef<HTMLSpanElement>(null);
  const play = useCallback(() => {
    const el = panel.current;
    if (!el || ms === 0) return;
    for (const a of el.getAnimations()) a.cancel();
    const total = 2 * ms + PREVIEW_HOLD_MS;
    const frames = reduced
      ? [{ width: "34%" }, { width: "34%" }]
      : [{ width: "0%" }, { width: "34%", offset: ms / total }, { width: "34%", offset: (ms + PREVIEW_HOLD_MS) / total }, { width: "0%" }];
    el.animate(frames, { duration: total, easing: "cubic-bezier(0.2, 0, 0, 1)" });
  }, [ms, reduced]);
  useEffect(play, [play]);
  return (
    <span
      aria-hidden
      data-testid="setting-panelMotion-preview"
      onMouseEnter={play}
      className="flex h-10 w-[72px] overflow-hidden rounded-md border border-control bg-bg"
    >
      <span ref={panel} className="h-full w-0 shrink-0 overflow-hidden bg-sidebar shadow-[inset_-1px_0_var(--color-control)]">
        <span className="flex w-6 flex-col gap-1 px-1 py-1.5">
          <span className="h-0.5 w-full rounded-full bg-control" />
          <span className="h-0.5 w-3/4 rounded-full bg-control" />
          <span className="h-0.5 w-full rounded-full bg-control" />
        </span>
      </span>
      <span className="flex flex-1 flex-col gap-1 px-1.5 py-1.5">
        <span className="h-0.5 w-1/3 rounded-full bg-run" />
        <span className="h-0.5 w-full rounded-full bg-control" />
        <span className="h-0.5 w-2/3 rounded-full bg-control" />
      </span>
    </span>
  );
}

function PanelMotionControl({ ms, reduced }: { ms: number; reduced: boolean }) {
  const dragged = useRef(false);
  const commit = () => {
    if (!dragged.current) return;
    dragged.current = false;
    void savePanelMotion(useWorkspace.getState().settings.panelMotionMs);
  };
  return (
    <>
      <MotionPreview ms={ms} reduced={reduced} />
      <span className="w-14 rounded-md bg-pill py-1 text-center font-mono text-ui-11.5 text-fg tabular-nums">{ms === 0 ? "Off" : `${ms} ms`}</span>
      <input
        data-testid="setting-panelMotion"
        aria-label="Panel animation duration"
        aria-valuetext={ms === 0 ? "Off" : `${ms} milliseconds`}
        type="range"
        min={0}
        max={500}
        step={25}
        value={ms}
        onChange={(e) => {
          dragged.current = true;
          previewPanelMotion(Number(e.target.value));
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
        className="w-36 accent-run"
      />
    </>
  );
}

export function DensitySetting() {
  const density = useWorkspace((s) => s.settings.density);
  return (
    <Row
      setting="density"
      hint="Spacing and row height across panels, lists, and result tables."
      control={<Segmented setting="density" choices={DENSITIES} value={density} onChoose={(d) => void chooseDensity(d)} />}
    />
  );
}

export function ReduceMotionSetting() {
  const reduceMotion = useWorkspace((s) => s.settings.reduceMotion);
  return (
    <Row
      setting="reduceMotion"
      hint="System follows the reduce motion setting of your OS. With motion reduced, panels open without sliding, menus and dialogs fade in place, and pulsing stops."
      control={<Segmented setting="reduceMotion" choices={REDUCE_MOTION} value={reduceMotion} onChoose={(r) => void chooseReduceMotion(r)} />}
    />
  );
}

export function PanelMotionSetting() {
  const panelMotionMs = useWorkspace((s) => s.settings.panelMotionMs);
  const reduced = useReducedMotion();
  const motionDefault = DEFAULT_SETTINGS.panelMotionMs;
  return (
    <Row
      setting="panelMotion"
      extra={
        panelMotionMs !== motionDefault && (
          <IconButton
            data-testid="setting-panelMotion-reset"
            aria-label="Reset panel animations"
            title={`Reset to ${motionDefault} ms`}
            className="size-5"
            onClick={() => void savePanelMotion(motionDefault)}
          >
            <Undo2Icon size={13} />
          </IconButton>
        )
      }
      hint={
        reduced
          ? "Motion is reduced, so panels open without sliding. Menus and dialogs fade for this long."
          : "How fast panels, dialogs, and menus open and close."
      }
      control={<PanelMotionControl ms={panelMotionMs} reduced={reduced} />}
    />
  );
}

export function TintConnectionsSetting() {
  const tintConnections = useWorkspace((s) => s.settings.tintConnections);
  return (
    <Row
      setting="tintConnections"
      labelFor="setting-tint-connections"
      hint="Shade each connection in the sidebar with its color instead of a dot."
      control={
        <input
          id="setting-tint-connections"
          data-testid="setting-tintConnections"
          type="checkbox"
          className="size-4 shrink-0 accent-run"
          checked={tintConnections}
          onChange={(e) => void saveSettings({ tintConnections: e.target.checked })}
        />
      }
    />
  );
}

function useFontProbe() {
  const [isInstalled] = useState(() => (typeof document === "undefined" ? noFonts : canvasFontProbe()));
  return isInstalled;
}

export function InterfaceFontSetting() {
  const font = useWorkspace((s) => s.settings.interfaceFont);
  const isInstalled = useFontProbe();
  return (
    <Row
      setting="interfaceFont"
      hint="Everything outside the SQL editor and result tables."
      control={<FontControls kind="interface" font={font} isInstalled={isInstalled} />}
    >
      <InterfaceSample font={font} />
    </Row>
  );
}

export function CodeFontSetting() {
  const font = useWorkspace((s) => s.settings.codeFont);
  const density = useWorkspace((s) => s.settings.density);
  const isInstalled = useFontProbe();
  return (
    <Row setting="codeFont" hint="SQL editor and result tables." control={<FontControls kind="code" font={font} isInstalled={isInstalled} />}>
      <CodeSample font={font} density={density} />
    </Row>
  );
}
