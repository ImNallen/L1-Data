import { useEffect, useRef, useState } from "react";
import { Group, Panel } from "react-resizable-panels";
import { ConnectionDialog } from "../features/connections/ConnectionDialog";
import { SqlEditor } from "../features/editor/SqlEditor";
import { StatementChooser } from "../features/editor/StatementChooser";
import { HistoryPalette } from "../features/history/HistoryPalette";
import { SavedPalette } from "../features/saved/SavedPalette";
import { ResultsPane } from "../features/results/ResultsPane";
import { SettingsDialog } from "../features/settings/SettingsScreen";
import { Sidebar } from "../features/sidebar/Sidebar";
import { InstallingOverlay } from "../features/updates/InstallingOverlay";
import { isTableTab, useWorkspace } from "../state/workspace";
import { Kbd } from "../ui/primitives";
import { START_COMMANDS, handleGlobalKey, shortcutLabel } from "./commands";
import { Overlays } from "./Overlays";
import { PixelColumn } from "./PixelColumn";
import { ResizeHandle } from "./ResizeHandle";
import { RightPanel } from "./RightPanel";
import { TabBar } from "./TabBar";
import { ResizeEdges } from "./WindowChrome";
import { hostedWindow } from "./hostedWindow";
import { TopBar } from "./TopBar";

function useGlobalKeys() {
  useEffect(() => {
    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, []);
}

function StartCommands() {
  return (
    <div data-testid="no-tabs" className="grid h-full place-items-center bg-bg">
      <ul className="flex w-[260px] flex-col gap-0.5">
        {START_COMMANDS.map((c) => (
          <li key={c.id}>
            <button
              type="button"
              data-testid={`start-${c.id}`}
              onClick={c.run}
              className="flex h-8 w-full items-center justify-between rounded-md px-2.5 text-ui-13.5 text-muted hover:bg-hover hover:text-fg"
            >
              {c.label}
              <Kbd className="bg-border text-muted">{shortcutLabel(c)}</Kbd>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

const TABLE_EDITOR_PX = 96;

function MainArea() {
  const active = useWorkspace((s) => (s.active ? s.tabs[s.active] : undefined));
  const tableTab = isTableTab(active);
  const showResults = useWorkspace((s) => s.layout.results);
  const resultsHeight = useWorkspace((s) => s.layout.resultsHeight);
  const resultsPx = useRef<number | null>(null);
  // Saved on release only: a store update mid-drag re-renders the Group and the library drops the drag.
  const saveResultsHeight = () => {
    const { layout, setLayout } = useWorkspace.getState();
    if (resultsPx.current !== null && resultsPx.current !== layout.resultsHeight) setLayout({ resultsHeight: resultsPx.current });
  };

  // A Panel reads defaultSize only on mount, so switching between a table tab and a query tab remounts the split.
  return (
    <Group key={tableTab ? "table" : "query"} orientation="vertical" className="min-h-0 flex-1" onLayoutChanged={tableTab ? undefined : saveResultsHeight}>
      <Panel
        id="editor"
        minSize={80}
        defaultSize={tableTab && showResults ? TABLE_EDITOR_PX : undefined}
        groupResizeBehavior={tableTab ? "preserve-pixel-size" : "preserve-relative-size"}
      >
        {active ? <SqlEditor tabId={active.id} /> : <StartCommands />}
      </Panel>
      {showResults && (
        <>
          <ResizeHandle id="results-resize" label="Resize results" line="horizontal" lineColor="bg-border" />
          <Panel
            id="results"
            defaultSize={tableTab ? undefined : resultsHeight}
            minSize={120}
            groupResizeBehavior={tableTab ? "preserve-relative-size" : "preserve-pixel-size"}
            onResize={(size) => {
              if (!tableTab) resultsPx.current = Math.round(size.inPixels);
            }}
          >
            <ResultsPane tabId={active?.id ?? null} />
          </Panel>
        </>
      )}
    </Group>
  );
}

export function AppLayout() {
  useGlobalKeys();
  const [hosted] = useState(() => hostedWindow());
  const sidebar = useWorkspace((s) => s.layout.sidebar);
  const rightPanel = useWorkspace((s) => s.layout.rightPanel);

  return (
    <div className="relative flex h-full">
      {hosted?.chrome === "custom" && <ResizeEdges hosted={hosted} />}
      <PixelColumn column="sidebar" open={sidebar}>
        <Sidebar hosted={hosted} />
      </PixelColumn>
      <div data-testid="main" className="flex min-w-80 flex-1 flex-col bg-bg">
        <TopBar hosted={hosted} />
        <TabBar />
        <MainArea />
      </div>
      <PixelColumn column="right" open={rightPanel !== null}>
        <RightPanel hosted={hosted} />
      </PixelColumn>
      <SettingsDialog />
      <HistoryPalette />
      <SavedPalette />
      <ConnectionDialog />
      <StatementChooser />
      <Overlays />
      <InstallingOverlay />
    </div>
  );
}
