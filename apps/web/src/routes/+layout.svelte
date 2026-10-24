<script lang="ts">
  import "@arclightning/ui/styles.css";
  import "@arclightning/ui/inter.css";
  import "virtual:uno.css";
  import { Icon, type IconName } from "@arclightning/ui";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import { adoptToken } from "$lib/token";

  let { children } = $props();

  const views: { href: string; label: string; icon: IconName }[] = [
    { href: "/inbox", label: "Inbox", icon: "inbox" },
    { href: "/", label: "Work", icon: "work" },
    { href: "/planning", label: "Planning", icon: "planning" },
  ];

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

<div class="shell">
  <nav class="sidebar" aria-label="Views">
    <a class="brand" href="/">Arc Lightning</a>
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
  <main>{@render children()}</main>
</div>

<style>
  .shell { display: grid; grid-template-columns: 15rem 1fr; min-height: 100vh; }
  .sidebar { display: grid; align-content: start; gap: var(--arcl-space-4); padding: var(--arcl-space-4) var(--arcl-space-3); border-right: var(--arcl-border-width) solid var(--arcl-border); background: var(--arcl-surface); font-family: var(--arcl-font-ui); }
  .brand { padding: 0 var(--arcl-space-2); color: var(--arcl-heading); font-weight: 650; text-decoration: none; }
  ul { display: grid; gap: var(--arcl-space-1); margin: 0; padding: 0; list-style: none; }
  li a { display: flex; align-items: center; gap: var(--arcl-space-2); padding: var(--arcl-space-1) var(--arcl-space-2); border-radius: var(--arcl-radius-sm); color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); text-decoration: none; }
  li a:hover { background: var(--arcl-surface-hover); color: var(--arcl-text); }
  li a[aria-current="page"] { background: var(--arcl-surface-subtle); color: var(--arcl-heading); font-weight: 600; }
  main { padding: var(--arcl-space-5) var(--arcl-space-6); }
</style>
