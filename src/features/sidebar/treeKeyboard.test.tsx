// @vitest-environment jsdom
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { buildCatalogView, useCatalog } from "../../state/catalog";
import { dbKey } from "../../state/dbKey";
import { useLive } from "../../state/live";
import { useWorkspace } from "../../state/workspace";
import { connectedState, connectionConfig, databaseState } from "../../testing/connections";
import { shopCatalog } from "../../testing/shopCatalog";
import { dropEditor, seedEditor, textOf } from "../editor/editorStates";
import { Sidebar } from "./Sidebar";

const shop = { connectionId: "p", database: "shop" };

const ROW_H = 28;
let viewport = 600;

// jsdom lays nothing out. Each row measures 28px and the tree's viewport `viewport`, so the virtualized tree leaves
// the rows past its overscan out of the DOM, as a short sidebar does.
beforeAll(() => {
  Element.prototype.scrollIntoView = () => {};
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.hasAttribute("data-index") ? ROW_H : viewport;
  });
});

beforeEach(() => {
  mockIPC((cmd) => {
    if (cmd === "table_script") return { text: "SELECT * FROM public.orders", acceptedTexts: [] };
    throw "not served in this test";
  });
  const view = buildCatalogView(shopCatalog("p", 1));
  useWorkspace.setState({ configs: { p: connectionConfig("p", "prod") } });
  useLive.setState({ connections: { p: connectedState("p", [databaseState("shop", true, ["public"])]) } });
  useCatalog.setState({ views: { [dbKey(view.catalog.db)]: view } });
});

afterEach(() => {
  viewport = 600;
  cleanup();
  clearMocks();
  dropEditor("t1");
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useLive.setState(useLive.getInitialState(), true);
  useCatalog.setState(useCatalog.getInitialState(), true);
});

const row = (path: string) => screen.getByTestId(`tree-node-${path}`);
const focusedPath = () => document.activeElement?.getAttribute("data-testid")?.replace("tree-node-", "");
const tabStops = () => screen.getAllByRole("treeitem").filter((r) => r.tabIndex === 0);

it("is one Tab stop that starts on the first row and returns to the row last focused", async () => {
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  expect(tabStops()).toEqual([row("prod")]);
  expect(row("prod").getAttribute("aria-level")).toBe("1");
  expect(row("prod/shop/public/orders").getAttribute("aria-level")).toBe("4");

  screen.getByTestId("new-connection").focus();
  await user.tab();
  expect(focusedPath()).toBe("prod");

  await user.keyboard("{ArrowDown}{ArrowDown}");
  expect(focusedPath()).toBe("prod/shop/public");
  await user.tab();
  expect(document.activeElement).toBe(screen.getByTestId("open-settings"));
  await user.tab({ shift: true });
  expect(focusedPath()).toBe("prod/shop/public");
  expect(tabStops()).toEqual([row("prod/shop/public")]);
});

it("moves with the arrows, Home and End, and opens and closes rows with Right and Left", async () => {
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  row("prod/shop/public/orders").focus();

  await user.keyboard("{ArrowRight}");
  expect(row("prod/shop/public/orders").getAttribute("aria-expanded")).toBe("true");
  expect(row("prod/shop/public/orders/#columns/status")).toBeTruthy();
  await user.keyboard("{ArrowRight}");
  expect(focusedPath()).toBe("prod/shop/public/orders/#columns");
  await user.keyboard("{ArrowDown}");
  expect(focusedPath()).toBe("prod/shop/public/orders/#columns/id");

  await user.keyboard("{ArrowLeft}");
  expect(focusedPath()).toBe("prod/shop/public/orders/#columns");
  await user.keyboard("{ArrowLeft}");
  expect(screen.queryByTestId("tree-node-prod/shop/public/orders/#columns/id")).toBeNull();
  await user.keyboard("{ArrowLeft}{ArrowLeft}");
  expect(focusedPath()).toBe("prod/shop/public/orders");
  expect(row("prod/shop/public/orders").getAttribute("aria-expanded")).toBe("false");

  await user.keyboard("{End}");
  expect(focusedPath()).toBe("prod/shop/public/#functions");
  await user.keyboard("{Home}");
  expect(focusedPath()).toBe("prod");
  await user.keyboard("{ArrowUp}");
  expect(focusedPath()).toBe("prod");
});

it("reaches rows scrolled out of the rendered window, and keeps the focused row rendered", async () => {
  viewport = 100;
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  expect(screen.queryByTestId("tree-node-prod/shop/public/#functions")).toBeNull();
  row("prod").focus();

  await user.keyboard("{End}");
  expect(focusedPath()).toBe("prod/shop/public/#functions");
  expect(tabStops()).toEqual([row("prod/shop/public/#functions")]);
  expect(row("prod/shop/public/#functions").getAttribute("aria-posinset")).toBe("16");
  expect(row("prod/shop/public/#functions").getAttribute("aria-setsize")).toBe("16");
  expect(row("prod").getAttribute("aria-setsize")).toBe("1");

  await user.keyboard("{Home}");
  expect(focusedPath()).toBe("prod");
  await user.keyboard("r");
  expect(focusedPath()).toBe("prod/shop/public/reviews");
  await user.keyboard("{Home}s");
  expect(focusedPath()).toBe("prod/shop");
  await user.keyboard("s");
  expect(focusedPath()).toBe("prod/shop/public/shipments");
});

it("opens a table with Enter, as a double-click does", async () => {
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  row("prod/shop/public/orders").focus();
  await user.keyboard("{Enter}");
  await waitFor(() => expect(Object.values(useWorkspace.getState().tabs).map((t) => t.title)).toEqual(["orders"]));
  expect(useWorkspace.getState().selected).toEqual({ db: shop, schema: "public", relation: "orders" });
});

it("inserts a routine's template with Enter, and toggles a row that has no double-click action", async () => {
  useWorkspace.setState({
    tabs: { t1: { id: "t1", title: "Query", console: { db: shop, mode: "AutoCommit" }, table: null, file: null } },
    order: ["t1"],
    active: "t1",
  });
  seedEditor("t1", "");
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  row("prod/shop/public/#functions").focus();

  await user.keyboard("{Enter}");
  expect(row("prod/shop/public/#functions").getAttribute("aria-expanded")).toBe("true");
  await user.keyboard("{ArrowRight}");
  expect(focusedPath()).toBe("prod/shop/public/#functions/apply_coupon");
  await user.keyboard("{Enter}");
  expect(textOf("t1")).toContain("apply_coupon()");
});

it("walks the search results the same way, entered from the search box with Down", async () => {
  useWorkspace.setState({ treeQuery: "order" });
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  screen.getByTestId("sidebar-search").focus();

  await user.keyboard("{ArrowDown}");
  expect(focusedPath()).toBe("prod");
  await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
  expect(focusedPath()).toBe("prod/shop/public/order_items");
  await user.keyboard("{ArrowLeft}");
  expect(screen.queryByTestId("tree-node-prod/shop/public/order_items/#columns/order_id")).toBeNull();
  await user.keyboard("{ArrowLeft}");
  expect(focusedPath()).toBe("prod/shop/public");
  await user.keyboard("{ArrowLeft}");
  expect(screen.queryByTestId("tree-node-prod/shop/public/orders")).toBeNull();
  expect(row("prod/shop/public").getAttribute("aria-expanded")).toBe("false");
});

it("marks Production, Staging and Develop connections in the sidebar", () => {
  useWorkspace.setState({
    configs: {
      p: { ...connectionConfig("p", "prod"), environment: "Production" },
      s: { ...connectionConfig("s", "stage"), environment: "Staging" },
      l: { ...connectionConfig("l", "local"), environment: "Local" },
    },
  });
  render(<Sidebar hosted={null} />);
  expect(screen.getByTestId("env-icon-prod").getAttribute("aria-label")).toBe("Production");
  expect(screen.getByTestId("env-icon-stage").getAttribute("aria-label")).toBe("Staging");
  expect(screen.getByTestId("env-icon-local").getAttribute("aria-label")).toBe("Develop");
});
