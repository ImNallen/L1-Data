import { FileXIcon, TableIcon, XIcon } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
import { closeTab } from "../features/editor/closeTab";
import { isDirty, onBuffersChanged } from "../features/editor/editorStates";
import type { TabId } from "../ipc/bindings";
import { useLive } from "../state/live";
import { hasUncommittedWork } from "../state/status";
import { useWorkspace, type Tab } from "../state/workspace";
import { cx } from "../ui/primitives";

function fileTooltip(tab: Tab): string | undefined {
  if (!tab.file) return undefined;
  const { path } = tab.file.binding;
  switch (tab.file.health) {
    case "ok":
      return path;
    case "missing":
      return `${path} was moved or deleted. Saving writes it again.`;
    case "changed":
      return `${path} changed on disk. Saving asks before overwriting it.`;
    case "unreadable":
      return `L1 Data couldn't read ${path} again. Saving asks before overwriting it.`;
    default: {
      const unreachable: never = tab.file.health;
      return unreachable;
    }
  }
}

function TabItem({ id }: { id: TabId }) {
  const tab = useWorkspace((s) => s.tabs[id]);
  const active = useWorkspace((s) => s.active === id);
  const setActive = useWorkspace((s) => s.setActive);
  const renameTab = useWorkspace((s) => s.renameTab);
  const dirty = useLive((s) => hasUncommittedWork(s.consoles[id]));
  const aborted = useLive((s) => s.consoles[id]?.txn === "Aborted");
  const unsaved = useSyncExternalStore(onBuffersChanged, () => isDirty(id));
  const [editing, setEditing] = useState(false);
  if (!tab) return null;
  const missing = tab.file?.health === "missing";

  return (
    <div
      data-testid={`tab-${id}`}
      data-active={active}
      role="tab"
      aria-selected={active}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          void closeTab(id);
        } else setActive(id);
      }}
      onDoubleClick={() => !tab.file && setEditing(true)}
      title={fileTooltip(tab)}
      className={cx(
        "group relative flex h-7 max-w-[240px] min-w-0 shrink-0 cursor-default items-center gap-2 rounded-md pr-1 pl-2.5",
        active ? "bg-pill text-fg" : "text-dim hover:bg-hover hover:text-muted",
      )}
    >
      {tab.table ? (
        <TableIcon size={14} className="shrink-0 text-muted" />
      ) : (
        <span className="font-mono text-ui-9.5 font-bold tracking-tight text-badge">SQL</span>
      )}
      {editing ? (
        <input
          autoFocus
          defaultValue={tab.title}
          onFocus={(e) => e.currentTarget.select()}
          onBlur={(e) => {
            const title = e.currentTarget.value.trim();
            if (title) renameTab(id, title);
            setEditing(false);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setEditing(false);
          }}
          className="w-36 bg-transparent text-ui-13.5 text-fg outline-none"
        />
      ) : (
        <span className={cx("truncate text-ui-13.5", missing && "line-through")}>{tab.title}</span>
      )}
      {missing && <FileXIcon data-testid={`tab-file-missing-${id}`} aria-label="File missing" size={13} className="-ml-1 shrink-0 text-amber" />}
      {unsaved && (
        <span data-testid={`tab-unsaved-${id}`} aria-label="Unsaved changes" className="-ml-1 size-2 shrink-0 rounded-full border-[1.5px] border-focus" />
      )}
      <span className="relative grid size-5 shrink-0 place-items-center">
        {dirty && (
          <span
            data-testid={`tab-dirty-${id}`}
            role="img"
            aria-label={aborted ? "Transaction aborted" : "Transaction open"}
            className={cx("absolute size-1.5 rounded-full group-hover:hidden", aborted ? "bg-danger" : "bg-amber")}
          />
        )}
        <button
          type="button"
          aria-label="Close tab"
          data-testid={`tab-close-${id}`}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => void closeTab(id)}
          className={cx(
            "grid size-5 place-items-center rounded text-dim hover:bg-hover hover:text-fg",
            active && !dirty ? "visible" : "invisible group-hover:visible",
          )}
        >
          <XIcon size={13} />
        </button>
      </span>
    </div>
  );
}

export function TabBar() {
  const order = useWorkspace((s) => s.order);
  return (
    <div role="tablist" className="flex h-10 shrink-0 items-center gap-1 overflow-x-auto px-2 [scrollbar-width:none]">
      {order.map((id) => (
        <TabItem key={id} id={id} />
      ))}
    </div>
  );
}
