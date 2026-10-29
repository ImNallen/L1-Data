import { SwatchBookIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Appearance } from "../../ipc/bindings";
import { previewTheme } from "../../shell/theme";
import { APPEARANCE_LABEL, APPEARANCES, L1_VARIANTS, type ThemeDraft } from "../../themes/catalog";
import { formatHex, isDark, parseHex } from "../../themes/color";
import { completePalette, TOKEN_NAMES, type Token } from "../../themes/palette";
import { Button, cx, inputClass, Modal } from "../../ui/primitives";
import { addCustomThemes } from "./themeActions";

const SEEDS = ["background", "accent"] as const satisfies readonly Token[];
const ADVANCED = TOKEN_NAMES.filter((t) => t !== "background" && t !== "accent");

const tokenLabel = (token: Token) => token.replace(/[A-Z]/g, (c) => ` ${c.toLowerCase()}`).replace(/^./, (c) => c.toUpperCase());

function ColorField({ token, value, onChange }: { token: Token; value: string; onChange: (hex: string) => void }) {
  const [text, setText] = useState(value);
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    setShown(value);
    setText(value);
  }
  const valid = parseHex(text) !== null;
  const commit = (next: string) => {
    setText(next);
    const parsed = parseHex(next);
    if (parsed) onChange(formatHex(parsed));
  };
  const opaque = value.slice(0, 7);
  return (
    <div className="flex items-center justify-between gap-3 py-1 pl-2">
      <label htmlFor={`theme-color-${token}`} className="min-w-0 truncate text-ui-12.5 text-muted">
        {tokenLabel(token)}
      </label>
      <div className="flex shrink-0 items-center gap-2">
        <span className="relative size-6 overflow-hidden rounded-full border border-control" style={{ background: value }}>
          <input
            type="color"
            aria-label={`${tokenLabel(token)} picker`}
            value={opaque}
            onChange={(e) => commit(e.target.value + value.slice(7))}
            className="absolute inset-0 size-full cursor-pointer opacity-0"
          />
        </span>
        <div className="w-[104px]">
          <input
            id={`theme-color-${token}`}
            data-testid={`theme-color-${token}`}
            spellCheck={false}
            aria-invalid={!valid}
            value={text}
            onChange={(e) => commit(e.target.value)}
            onBlur={() => setText(value)}
            className={cx(inputClass, "h-7 font-mono text-ui-12", !valid && "border-danger focus:border-danger")}
          />
        </div>
      </div>
    </div>
  );
}

export function CreateThemePanel({ initial, onClose }: { initial: ThemeDraft; onClose: () => void }) {
  const [draft, setDraft] = useState(initial);
  const [advanced, setAdvanced] = useState(false);
  const nameInput = useRef<HTMLInputElement>(null);
  const palette = completePalette(draft.appearance, draft.colors);
  const name = draft.name.trim();

  useEffect(() => {
    previewTheme({
      id: "draft",
      name: draft.name,
      appearance: draft.appearance,
      swatch: { background: draft.colors.background, accent: draft.colors.accent },
      palette,
    });
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- `palette` derives from `draft`
  }, [draft]);
  useEffect(() => () => previewTheme(null), []);

  const setColor = (token: Token, value: string) => setDraft((d) => ({ ...d, colors: { ...d.colors, [token]: value } }));
  const setAppearance = (appearance: Appearance) =>
    setDraft((d) => {
      const fits = isDark(d.colors.background) === (appearance === "dark");
      const background = fits ? d.colors.background : L1_VARIANTS[appearance].swatch.background;
      return { ...d, appearance, colors: { ...d.colors, background } };
    });
  const create = async () => {
    if (!name) return;
    await addCustomThemes([{ name, variants: [{ ...draft, name }] }]);
    onClose();
  };

  return (
    <Modal
      open
      onOpenChange={(open) => !open && onClose()}
      title="Create theme"
      description="Two colors, the rest derived."
      width={440}
      testId="create-theme-panel"
      initialFocus={nameInput}
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-4">
          <label htmlFor="theme-name" className="shrink-0 text-ui-13 text-fg">
            Theme name
          </label>
          <div className="w-[210px]">
            <input
              ref={nameInput}
              id="theme-name"
              data-testid="theme-name"
              placeholder="e.g. Aurora"
              value={draft.name}
              onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
              onKeyDown={(e) => {
                if (e.key === "Enter") void create();
              }}
              className={inputClass}
            />
          </div>
        </div>
        <div className="flex items-center justify-between gap-4">
          <span id="theme-appearance" className="text-ui-13 text-fg">
            Appearance
          </span>
          <div role="radiogroup" aria-labelledby="theme-appearance" className="flex rounded-lg bg-pill p-0.5">
            {APPEARANCES.map((a) => (
              <button
                key={a}
                type="button"
                role="radio"
                aria-checked={draft.appearance === a}
                data-testid={`theme-appearance-${a}`}
                onClick={() => setAppearance(a)}
                className={cx("h-7 rounded-md px-3 text-ui-12.5 transition-colors", draft.appearance === a ? "bg-control text-fg" : "text-muted hover:text-fg")}
              >
                {APPEARANCE_LABEL[a]}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-start justify-between gap-4">
          <div>
            <span className="block text-ui-13 text-fg">Colors</span>
            <span className="block text-ui-12 text-dim">{advanced ? "Every token, editable" : "Background and accent"}</span>
          </div>
          <label className="flex items-center gap-2 text-ui-12.5 text-muted">
            Advanced
            <input
              type="checkbox"
              data-testid="theme-advanced"
              checked={advanced}
              onChange={(e) => setAdvanced(e.target.checked)}
              className="size-4 accent-run"
            />
          </label>
        </div>
        <div className="-mt-1 flex flex-col">
          {(advanced ? [...SEEDS, ...ADVANCED] : SEEDS).map((token) => (
            <ColorField key={token} token={token} value={palette[token]} onChange={(v) => setColor(token, v)} />
          ))}
        </div>
      </div>
      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button
          variant="primary"
          data-testid="create-theme-save"
          disabled={!name}
          title={name ? undefined : "Name the theme first"}
          onClick={() => void create()}
        >
          <SwatchBookIcon size={14} />
          Create theme
        </Button>
      </div>
    </Modal>
  );
}
