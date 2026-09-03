<script lang="ts">
  import type { PageProps } from "./$types";
  import { docs, getDoc } from "$lib/content";

  let { data }: PageProps = $props();
  const doc = $derived(getDoc(data.slug)!);
  const Content = $derived(doc.component);
</script>

<svelte:head><title>{doc.title} | Arc Lightning</title><meta name="description" content={doc.description} /></svelte:head>

<div class="docs-layout">
  <aside><nav aria-label="Documentation"><ul>{#each docs as item}<li><a href={`/${item.slug}/`} aria-current={item.slug === doc.slug ? "page" : undefined}>{item.title}</a></li>{/each}</ul></nav></aside>
  <main id="main-content" class="article"><p class="eyebrow">Documentation</p><h1>{doc.title}</h1><p class="article__lede">{doc.description}</p><div class="prose"><Content /></div></main>
</div>
