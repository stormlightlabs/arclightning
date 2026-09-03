<script lang="ts">
  import { Icon } from "@arclightning/ui";
  import SiteFooter from "$lib/components/SiteFooter.svelte";
  import SiteHeader from "$lib/components/SiteHeader.svelte";
  import { docs } from "$lib/content";

  const entryPoints = [
    { number: "01", label: "Get started", title: "Install and initialize", href: "/quick-start/" },
    { number: "02", label: "Plan work", title: "Understand the project model", href: "/overview/" },
    { number: "03", label: "Execute", title: "Work with tasks", href: "/guides/tasks/" },
  ];
</script>

<svelte:head>
  <title>Arc Lightning · Connected planning for developers and agents</title>
  <meta name="description" content="A local-first project planning and execution system for developers and software agents." />
  <meta property="og:title" content="Arc Lightning" />
  <meta property="og:description" content="Connected, local-first planning for developers and software agents." />
  <meta property="og:image" content="https://arclightning.stormlightlabs.org/social-card.svg" />
  <meta name="twitter:card" content="summary_large_image" />
  <link rel="canonical" href="https://arclightning.stormlightlabs.org/" />
</svelte:head>

<SiteHeader {docs} />
<main id="main-content" class="landing" data-pagefind-ignore>
  <section class="hero" aria-labelledby="landing-title">
    <div class="hero__copy">
      <p class="eyebrow">Local-first project planning</p>
      <h1 id="landing-title">Turn intent into ready work.</h1>
      <p class="hero__lede">Arc Lightning connects captures, specs, plans, tasks, and handoffs in one project model for developers and software agents.</p>
      <div class="hero__actions"><a class="button-link" href="/quick-start/">Get started <Icon name="arrowRight" size={18} /></a><a class="text-link" href="/overview/">How it works <Icon name="arrowRight" size={18} /></a></div>
    </div>
    <figure class="terminal" aria-labelledby="terminal-caption">
      <figcaption id="terminal-caption"><span class="terminal__dots" aria-hidden="true"><i></i><i></i><i></i></span><span>arcl · ready work</span></figcaption>
      <pre><code><span class="prompt">$</span> arcl capture create "Ship keyboard navigation"
<span class="success">created</span>  arcl-c-01J…

<span class="prompt">$</span> arcl task list --ready
<span class="meta">READY   HIGH</span>  Add focus-visible states
<span class="meta">READY   MED</span>   Document keyboard shortcuts

<span class="prompt">$</span> arcl context arcl-t-01J…
<span class="accent">spec</span>      Keyboard navigation
<span class="accent">handoff</span>   Continue from the dialog focus trap</code></pre>
    </figure>
  </section>

  <section class="workflow" aria-labelledby="workflow-title">
    <header><p class="eyebrow">One connected model</p><h2 id="workflow-title">Plan only as much as the work needs.</h2></header>
    <ol>
      <li><span>Capture</span><div><h3>Start before the shape is clear.</h3><p>Save a thought, then promote it to a spec, task, or note without losing where it came from.</p></div></li>
      <li><span>Plan</span><div><h3>Connect the why to the work.</h3><p>Keep owned Markdown specs beside persistent plans, optional phases, and executable tasks.</p></div></li>
      <li><span>Execute</span><div><h3>Know what is ready—and why.</h3><p>Deterministic blockers, focused context, handoffs, and evidence keep people and agents aligned.</p></div></li>
    </ol>
  </section>

  <section class="local-first" aria-labelledby="local-title"><div><p class="eyebrow">Your project stays yours</p><h2 id="local-title">Local by default. Git when you want it.</h2></div><p>Arc Lightning works in any directory with SQLite as its operational store. Opt into readable project files for review and collaboration without requiring an account or hosted service.</p></section>

  <section class="landing-docs" aria-labelledby="docs-title"><header><p class="eyebrow">Documentation</p><h2 id="docs-title">Start with a workflow.</h2></header><nav aria-label="Documentation entry points">{#each entryPoints as entry}<a href={entry.href}><span class="doc-number">{entry.number}</span><span><small>{entry.label}</small><strong>{entry.title}</strong></span><Icon name="arrowRight" size={18} /></a>{/each}</nav></section>
</main>
<SiteFooter />
