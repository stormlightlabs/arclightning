/** The local storage key that remembers whether the sidebar is collapsed. */
export const SIDEBAR_STORAGE_KEY = "arcl-sidebar";

/** The key that collapses and expands the sidebar, beside `]` for the properties panel. */
export const SIDEBAR_KEY = "[";

/**
 * Whether the sidebar was left collapsed. `storage` is a function because
 * reading `window.localStorage` itself can throw, as it does on a sandboxed
 * artifact page; any failure reads as expanded.
 */
export function sidebarCollapsed(storage: () => Pick<Storage, "getItem">): boolean {
  try {
    return storage().getItem(SIDEBAR_STORAGE_KEY) === "collapsed";
  } catch {
    return false;
  }
}

/** Remember whether the sidebar is collapsed, when `storage` allows it. */
export function rememberSidebar(storage: () => Pick<Storage, "setItem">, collapsed: boolean): void {
  try {
    storage().setItem(SIDEBAR_STORAGE_KEY, collapsed ? "collapsed" : "expanded");
  } catch {
    // Without storage the choice lasts until the page reloads.
  }
}
