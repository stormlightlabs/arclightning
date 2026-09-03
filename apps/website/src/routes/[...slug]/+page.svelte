<script lang="ts">
  import type { PageProps } from "./$types";
  import DocsShell from "$lib/components/DocsShell.svelte";
  import { getDoc } from "$lib/content";

  let { data }: PageProps = $props();
  const doc = $derived(getDoc(data.slug)!);
  const Content = $derived(doc.component);
</script>

<DocsShell {doc}>{#snippet content()}<Content />{/snippet}</DocsShell>
