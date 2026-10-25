<script lang="ts">
  import "@arclightning/ui/styles.css";
  import "@arclightning/ui/inter.css";
  import "virtual:uno.css";
  import { Button, CommandMenu, EmptyState, Feedback, Icon, type IconName } from "@arclightning/ui";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import QuickCapture from "$lib/capture/QuickCapture.svelte";
  import {
    SEQUENCE_TIMEOUT,
    TASK_RESULT_PREFIX,
    boundCommands,
    formatKeys,
    isMac,
    menuGroups,
    opensMenu,
    registeredCommands,
    registeredContext,
    shortcutStep,
    takesShortcut,
    useCommands,
    type Command,
  } from "$lib/commands";
  import { ARTIFACT_BUILD, OPEN_IN_CLAUDE, connectArtifact, dataSource, type ChangeWarning, type SyncStatus as Sync, type TaskSummary } from "$lib/data";
  import { appHref, appUrl } from "$lib/route";
  import SyncStatus from "$lib/sync/SyncStatus.svelte";
  import { adoptToken } from "$lib/token";
  import { taskHref } from "$lib/work";

  let { children } = $props();

  const views: { path: string; label: string; icon: IconName; key: string }[] = [
    { path: "/inbox", label: "Inbox", icon: "inbox", key: "i" },
    { path: "/", label: "Work", icon: "work", key: "w" },
    { path: "/planning", label: "Planning", icon: "planning", key: "p" },
  ];
  const mac = isMac(navigator.platform);
  const menuKeys = formatKeys(["Mod", "k"], mac);

  let menuOpen = $state(false);
  let query = $state("");
  /** Whether Backspace removed the selection's chip for this opening. */
  let contextCleared = $state(false);
  let tasks = $state<TaskSummary[]>([]);
  let captureOpen = $state(false);
  /** Whether the views menu is open. Only narrow windows hide it. */
  let navOpen = $state(false);
  /** What the last quick capture filed, announced until the next one. */
  let filed = $state<string | null>(null);
  /** The warnings of the last quick capture, shown until dismissed. */
  let captureWarnings = $state<ChangeWarning[]>([]);
  /** The sidebar, which a narrow window shows as the top bar. */
  let nav = $state<HTMLElement>();
  /** The Menu button, which Escape returns focus to. */
  let navToggle = $state<HTMLButtonElement>();
  /** Whether views can read yet. The artifact build waits for the artifact's database. */
  let connection = $state<"ready" | "connecting" | "unavailable">(ARTIFACT_BUILD ? "connecting" : "ready");
  /** The artifact's last sync and queued edits; `null` in the `arcl serve` build. */
  let sync = $state<Sync | null>(null);
  /** The revision of the records the views loaded. */
  let shownRevision = $state<string | null>(null);
  let syncedRevision = $derived(sync?.revision ?? null);
  /** Bumped to mount the view again, so it reads the records a sync brought. */
  let viewKey = $state(0);
  let pending: string[] = [];
  let pendingTimer: ReturnType<typeof setTimeout> | undefined;

  let context = $derived(contextCleared ? null : registeredContext());
  let groups = $derived(menuGroups({ commands: registeredCommands(), query, context, tasks, mac }));

  useCommands(() => ({
    commands: [
      { id: "global:capture", label: "Quick capture", scope: "global", group: "Create", keys: ["q"], icon: "capture", run: () => (captureOpen = true) },
      ...views.map((view): Command => ({
        id: `global:go-${view.key}`,
        label: view.label,
        scope: "global",
        group: "Go to",
        keys: ["g", view.key],
        icon: view.icon,
        run: () => goto(appHref(view.path)),
      })),
    ],
  }));

  if (ARTIFACT_BUILD) connectArtifact().then((connected) => (connection = connected ? "ready" : "unavailable"));

  // Follow the artifact's syncs and queue once its database is connected.
  $effect(() => {
    if (connection !== "ready") return;
    return dataSource().watch?.((status) => {
      shownRevision ??= status.revision;
      sync = status;
    });
  });

  function reloadView() {
    shownRevision = syncedRevision;
    viewKey += 1;
  }

  /** Remove a failed edit from the queue. One that stays shows on the next update. */
  function dismiss(id: string) {
    dataSource()
      .dismiss?.(id)
      .catch(() => {});
  }

  /** Open the command menu with an empty search, and load the tasks it searches. */
  function openMenu() {
    query = "";
    contextCleared = false;
    menuOpen = true;
    dataSource()
      .taskIndex()
      .then((loaded) => (tasks = loaded))
      .catch(() => (tasks = []));
  }

  function runCommand(id: string) {
    if (id.startsWith(TASK_RESULT_PREFIX)) {
      goto(taskHref(id.slice(TASK_RESULT_PREFIX.length)));
      return;
    }
    registeredCommands().find((command) => command.id === id)?.run();
  }

  function onkeydown(event: KeyboardEvent) {
    if (opensMenu(event, mac)) {
      event.preventDefault();
      openMenu();
      return;
    }
    if (menuOpen || captureOpen || !takesShortcut(event)) return;
    const step = shortcutStep(pending, event.key, boundCommands(registeredCommands(), registeredContext()));
    pending = step.pending;
    clearTimeout(pendingTimer);
    if (pending.length) pendingTimer = setTimeout(() => (pending = []), SEQUENCE_TIMEOUT);
    if (step.match === null && !pending.length) return;
    event.preventDefault();
    step.match?.run();
  }

  /**
   * Close the views menu on Escape pressed inside the bar, and return focus to
   * its toggle. This runs in the capture phase and prevents the default, so the
   * view's own Escape, such as the Task view's "back", does not run too. Escape
   * pressed anywhere else, such as in an editor or a select in the view, keeps
   * its own meaning.
   */
  function closeNavOnEscape(event: KeyboardEvent) {
    if (event.key !== "Escape" || !navOpen || event.defaultPrevented) return;
    if (!(event.target instanceof Node) || !nav?.contains(event.target) || !navToggle?.checkVisibility()) return;
    event.preventDefault();
    navOpen = false;
    navToggle.focus();
  }

  /** Close the views menu when a click lands outside the bar. */
  function closeNavOutside(event: PointerEvent) {
    if (navOpen && event.target instanceof Node && !nav?.contains(event.target)) navOpen = false;
  }

  /**
   * Close the views menu when focus moves to an element outside the bar, so it
   * never stays open while the view has focus. Focus moving into a dialog,
   * such as the one Search opens, leaves it open, so closing the dialog can
   * return focus to the button that opened it. So does focus leaving the page,
   * which has no `relatedTarget`.
   */
  function closeNavOnFocusOut(event: FocusEvent) {
    const to = event.relatedTarget;
    if (navOpen && to instanceof Element && !nav?.contains(to) && !to.closest("dialog")) navOpen = false;
  }

  // Store the token before any view mounts and reads data. Drop it from the
  // URL once the first navigation finishes, since the router refuses to
  // navigate before that. A real navigation, unlike shallow `replaceState`,
  // updates `page.url` too, so views that rewrite the query do not bring the
  // token back.
  // The artifact build has no token, and a sandboxed artifact page may have no
  // session storage.
  let cleaned = ARTIFACT_BUILD ? null : adoptToken(new URL(window.location.href), window.sessionStorage);
  afterNavigate(() => {
    navOpen = false;
    const target = cleaned;
    cleaned = null;
    if (target) goto(target, { replaceState: true, keepFocus: true, noScroll: true });
  });
</script>

<svelte:window {onkeydown} onkeydowncapture={closeNavOnEscape} onpointerdown={closeNavOutside} />

<div class="shell">
  <nav class="sidebar" aria-label="Views" bind:this={nav} onfocusout={closeNavOnFocusOut}>
    <div class="bar">
      <a class="brand" href={appHref("/")}>Arc Lightning</a>
      <button type="button" class="menu-toggle" bind:this={navToggle} aria-expanded={navOpen} aria-controls="views-menu" onclick={() => (navOpen = !navOpen)}>
        <Icon name={navOpen ? "close" : "menu"} size="1.125rem" />
        <span>Menu</span>
      </button>
    </div>
    <div class="menu" id="views-menu" data-open={navOpen ? "" : undefined}>
      <div class="tools">
        <button type="button" class="tool" aria-keyshortcuts={mac ? "Meta+K" : "Control+K"} onclick={openMenu}>
          <Icon name="search" size="1rem" />
          <span>Search</span>
          <span class="keys" aria-hidden="true">{#each menuKeys as key (key)}<kbd>{key}</kbd>{/each}</span>
        </button>
        <button type="button" class="tool" aria-keyshortcuts="Q" onclick={() => (captureOpen = true)}>
          <Icon name="capture" size="1rem" />
          <span>Capture</span>
          <span class="keys" aria-hidden="true"><kbd>Q</kbd></span>
        </button>
      </div>
      <ul>
        {#each views as view (view.path)}
          <li>
            <a href={appHref(view.path)} aria-current={appUrl(page.url).pathname === view.path ? "page" : undefined}>
              <Icon name={view.icon} size="1.125rem" />
              {view.label}
            </a>
          </li>
        {/each}
      </ul>
    </div>
  </nav>
  <main>
    {#if filed !== null}<p class="filed" role="status">{filed}</p>{/if}
    {#each captureWarnings as warning, index (index)}
      <Feedback tone="warning" title="The capture is saved, with a warning" message={`${warning.message} (${warning.code})`}>
        {#snippet actions()}
          <Button label="Dismiss" variant="ghost" onclick={() => (captureWarnings = captureWarnings.filter((_, other) => other !== index))} />
        {/snippet}
      </Feedback>
    {/each}
    {#if sync !== null}
      <SyncStatus status={sync} stale={syncedRevision !== shownRevision} onreload={reloadView} ondismiss={dismiss} />
    {/if}
    {#if connection === "connecting"}
      <p class="connecting" role="status">Connecting to the artifact’s database…</p>
    {:else if connection === "unavailable"}
      <EmptyState title="Open this page in claude.ai" message={OPEN_IN_CLAUDE} />
    {:else}
      {#key viewKey}{@render children()}{/key}
    {/if}
  </main>
</div>

<CommandMenu
  bind:open={menuOpen}
  bind:query
  {groups}
  context={context?.label}
  onclearcontext={() => (contextCleared = true)}
  onrun={runCommand}
/>
<QuickCapture
  bind:open={captureOpen}
  {mac}
  oncaptured={(capture, warnings) => ((filed = `Captured “${capture.title}” in the Inbox.`), (captureWarnings = warnings))}
  oncreated={(task, warnings) => ((filed = `Created “${task.title}”.`), (captureWarnings = []), goto(taskHref(task.id), { state: { warnings } }))}
/>

<style>
  .shell { display: grid; grid-template-columns: 15rem 1fr; min-height: 100vh; }
  .sidebar { display: grid; align-content: start; gap: var(--arcl-space-4); padding: var(--arcl-space-4) var(--arcl-space-3); border-right: var(--arcl-border-width) solid var(--arcl-border); background: var(--arcl-surface); font-family: var(--arcl-font-ui); }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: var(--arcl-space-2); }
  .menu { display: grid; gap: var(--arcl-space-4); }
  .menu-toggle { display: none; align-items: center; gap: var(--arcl-space-1); padding: var(--arcl-space-1) var(--arcl-space-2); border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-sm); color: var(--arcl-text); background: var(--arcl-canvas); font: inherit; font-size: var(--arcl-type-sm); cursor: pointer; }
  .brand { padding: 0 var(--arcl-space-2); color: var(--arcl-heading); font-weight: 650; text-decoration: none; }
  ul { display: grid; gap: var(--arcl-space-1); margin: 0; padding: 0; list-style: none; }
  li a { display: flex; align-items: center; gap: var(--arcl-space-2); padding: var(--arcl-space-1) var(--arcl-space-2); border-radius: var(--arcl-radius-sm); color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); text-decoration: none; }
  li a:hover { background: var(--arcl-surface-hover); color: var(--arcl-text); }
  li a[aria-current="page"] { background: var(--arcl-surface-subtle); color: var(--arcl-heading); font-weight: 600; }
  main { min-width: 0; padding: var(--arcl-space-5) var(--arcl-space-6); }
  .tools { display: grid; gap: var(--arcl-space-1); }
  .tool { display: flex; align-items: center; gap: var(--arcl-space-2); padding: var(--arcl-space-1) var(--arcl-space-2); border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-sm); color: var(--arcl-text-muted); background: var(--arcl-canvas); font: inherit; font-size: var(--arcl-type-sm); text-align: start; cursor: pointer; }
  .tool:hover { background: var(--arcl-surface-hover); color: var(--arcl-text); }
  .tool span:not(.keys) { flex: 1; }
  .keys { display: inline-flex; gap: 2px; }
  kbd { display: inline-flex; min-width: 1.125rem; height: 1.125rem; align-items: center; justify-content: center; padding: 0 0.25rem; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: 4px; background: var(--arcl-surface); font: 500 var(--arcl-type-xs) / 1 var(--arcl-font-ui); }
  .connecting { margin: 0; color: var(--arcl-text-muted); }
  .filed { margin: 0 0 var(--arcl-space-4); padding: var(--arcl-space-2) var(--arcl-space-3); border-inline-start: 4px solid var(--arcl-success); border-radius: var(--arcl-radius-sm); background: var(--arcl-success-surface); font-size: var(--arcl-type-sm); }
  /* Narrow windows stack the sidebar above the page as a bar whose Menu button shows the views. */
  @media (max-width: 48rem) {
    /* The bar's height, border included. The page scrolls focused rows below it. */
    :global(:root) { --shell-bar-height: 3rem; scroll-padding-top: var(--shell-bar-height); }
    .shell { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto 1fr; }
    .sidebar { position: sticky; top: 0; z-index: 1; padding: 0 var(--arcl-space-3); border-right: 0; border-bottom: var(--arcl-border-width) solid var(--arcl-border); }
    .bar { min-height: calc(var(--shell-bar-height) - var(--arcl-border-width)); }
    .menu-toggle { display: inline-flex; }
    .menu:not([data-open]) { display: none; }
    .menu[data-open] { padding-bottom: var(--arcl-space-2); }
    main { padding: var(--arcl-space-4) var(--arcl-space-3); }
  }
</style>
