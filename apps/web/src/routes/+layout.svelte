<script lang="ts">
  import "@arclightning/ui/styles.css";
  import "@arclightning/ui/inter.css";
  import "virtual:uno.css";
  import { CommandMenu, Icon, type IconName } from "@arclightning/ui";
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
  import { dataSource, type TaskSummary } from "$lib/data";
  import { adoptToken } from "$lib/token";
  import { taskHref } from "$lib/work";

  let { children } = $props();

  const views: { href: string; label: string; icon: IconName; key: string }[] = [
    { href: "/inbox", label: "Inbox", icon: "inbox", key: "i" },
    { href: "/", label: "Work", icon: "work", key: "w" },
    { href: "/planning", label: "Planning", icon: "planning", key: "p" },
  ];
  const mac = isMac(navigator.platform);
  const menuKeys = formatKeys(["Mod", "k"], mac);

  let menuOpen = $state(false);
  let query = $state("");
  /** Whether Backspace removed the selection's chip for this opening. */
  let contextCleared = $state(false);
  let tasks = $state<TaskSummary[]>([]);
  let captureOpen = $state(false);
  /** What the last quick capture filed, announced until the next one. */
  let filed = $state<string | null>(null);
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
        run: () => goto(view.href),
      })),
    ],
  }));

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

  // Store the token before any view mounts and reads data. Drop it from the
  // URL once the first navigation finishes, since the router refuses to
  // navigate before that. A real navigation, unlike shallow `replaceState`,
  // updates `page.url` too, so views that rewrite the query do not bring the
  // token back.
  let cleaned = adoptToken(new URL(window.location.href), window.sessionStorage);
  afterNavigate(() => {
    const target = cleaned;
    cleaned = null;
    if (target) goto(target, { replaceState: true, keepFocus: true, noScroll: true });
  });
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <nav class="sidebar" aria-label="Views">
    <a class="brand" href="/">Arc Lightning</a>
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
      {#each views as view (view.href)}
        <li>
          <a href={view.href} aria-current={page.url.pathname === view.href ? "page" : undefined}>
            <Icon name={view.icon} size="1.125rem" />
            {view.label}
          </a>
        </li>
      {/each}
    </ul>
  </nav>
  <main>
    {#if filed !== null}<p class="filed" role="status">{filed}</p>{/if}
    {@render children()}
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
  oncaptured={(capture) => (filed = `Captured “${capture.title}” in the Inbox.`)}
  oncreated={(task) => ((filed = `Created “${task.title}”.`), goto(taskHref(task.id)))}
/>

<style>
  .shell { display: grid; grid-template-columns: 15rem 1fr; min-height: 100vh; }
  .sidebar { display: grid; align-content: start; gap: var(--arcl-space-4); padding: var(--arcl-space-4) var(--arcl-space-3); border-right: var(--arcl-border-width) solid var(--arcl-border); background: var(--arcl-surface); font-family: var(--arcl-font-ui); }
  .brand { padding: 0 var(--arcl-space-2); color: var(--arcl-heading); font-weight: 650; text-decoration: none; }
  ul { display: grid; gap: var(--arcl-space-1); margin: 0; padding: 0; list-style: none; }
  li a { display: flex; align-items: center; gap: var(--arcl-space-2); padding: var(--arcl-space-1) var(--arcl-space-2); border-radius: var(--arcl-radius-sm); color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); text-decoration: none; }
  li a:hover { background: var(--arcl-surface-hover); color: var(--arcl-text); }
  li a[aria-current="page"] { background: var(--arcl-surface-subtle); color: var(--arcl-heading); font-weight: 600; }
  main { padding: var(--arcl-space-5) var(--arcl-space-6); }
  .tools { display: grid; gap: var(--arcl-space-1); }
  .tool { display: flex; align-items: center; gap: var(--arcl-space-2); padding: var(--arcl-space-1) var(--arcl-space-2); border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-sm); color: var(--arcl-text-muted); background: var(--arcl-canvas); font: inherit; font-size: var(--arcl-type-sm); text-align: start; cursor: pointer; }
  .tool:hover { background: var(--arcl-surface-hover); color: var(--arcl-text); }
  .tool span:not(.keys) { flex: 1; }
  .keys { display: inline-flex; gap: 2px; }
  kbd { display: inline-flex; min-width: 1.125rem; height: 1.125rem; align-items: center; justify-content: center; padding: 0 0.25rem; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: 4px; background: var(--arcl-surface); font: 500 var(--arcl-type-xs) / 1 var(--arcl-font-ui); }
  .filed { margin: 0 0 var(--arcl-space-4); padding: var(--arcl-space-2) var(--arcl-space-3); border-inline-start: 4px solid var(--arcl-success); border-radius: var(--arcl-radius-sm); background: var(--arcl-success-surface); font-size: var(--arcl-type-sm); }
</style>
