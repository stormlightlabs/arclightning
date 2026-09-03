<script lang="ts">
  import "../styles/theme.css";
  let { children } = $props();
</script>

<svelte:head><link rel="icon" href="/favicon.svg" /></svelte:head>
<a class="skip-link" href="#main-content">Skip to content</a>
<header class="site-header">
  <a class="site-header__brand" href="/">Arc Lightning</a>
  <nav aria-label="Primary navigation"><a href="/overview/">Overview</a><a href="/quick-start/">Quick start</a><a href="https://github.com/stormlightlabs/arclightning">GitHub</a></nav>
</header>
{@render children()}
