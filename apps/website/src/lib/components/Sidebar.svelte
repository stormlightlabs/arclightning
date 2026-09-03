<script lang="ts">
  import { docSections, type Doc } from "$lib/content";
  let { docs, currentSlug }: { docs: Doc[]; currentSlug: string } = $props();
</script>

<div class="sidebar__inner">
  <p class="sidebar__kicker">Documentation</p>
  {#each docSections as section}
    <section class="sidebar__section" aria-labelledby={`sidebar-${section.toLowerCase().replace(" ", "-")}`}>
      <h2 id={`sidebar-${section.toLowerCase().replace(" ", "-")}`}>{section}</h2>
      <nav aria-label={`${section} pages`}>{#each docs.filter((doc) => doc.section === section) as doc}<a href={`/${doc.slug}/`} class:active={doc.slug === currentSlug} aria-current={doc.slug === currentSlug ? "page" : undefined}>{doc.title}</a>{/each}</nav>
    </section>
  {/each}
</div>
