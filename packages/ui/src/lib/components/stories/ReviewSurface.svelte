<script lang="ts">
  import { createRawSnippet } from "svelte";
  import Button from "../Button.svelte";
  import EmptyState from "../EmptyState.svelte";
  import Feedback from "../Feedback.svelte";
  import Field from "../Field.svelte";
  import IconButton from "../IconButton.svelte";
  import Markdown from "../Markdown.svelte";
  import Menu, { type MenuItem } from "../Menu.svelte";
  import PlanningTree from "../PlanningTree.svelte";
  import PriorityBadge from "../PriorityBadge.svelte";
  import Readiness from "../Readiness.svelte";
  import RecordSummary from "../RecordSummary.svelte";
  import Select from "../Select.svelte";
  import StatusBadge from "../StatusBadge.svelte";
  import Tabs, { type TabItem } from "../Tabs.svelte";
  import type { PlanningRecordSummary, PlanningTreeNode } from "../../types";

  let { theme = "light" }: { theme?: "light" | "dark" } = $props();
  let selected = $state("task-1");
  let title = $state("Recover interrupted workspace export");

  const menuItems: MenuItem[] = [
    { id: "promote", label: "Promote to task" },
    { id: "discard", label: "Discard capture", destructive: true },
    { id: "archive", label: "Archive", disabled: true },
  ];
  const task: PlanningRecordSummary = {
    id: "task-1", kind: "task", title: "Recover interrupted workspace export",
    description: "Compare the database projection with the last common files before writing.",
    status: "ready", priority: "high", metadata: "T09 · updated 14:32",
  };
  const tree: PlanningTreeNode[] = [{
    id: "spec-1", kind: "spec", title: "Repository-native collaboration", status: "draft",
    children: [{ id: "plan-1", kind: "plan", title: "Safe bidirectional synchronization", status: "in_progress", children: [task] }],
  }];
  const backlog = createRawSnippet(() => ({ render: () => "<p>Tasks that still need sequencing.</p>" }));
  const ready = createRawSnippet(() => ({ render: () => "<p>Actionable tasks with every blocker complete.</p>" }));
  const tabs: TabItem[] = [{ id: "backlog", label: "Backlog", panel: backlog }, { id: "ready", label: "Ready", panel: ready }];
</script>

<div class="review" data-arcl-theme={theme}>
  <header><p>Arc Lightning UI</p><h1>Shared component review</h1><span>Theme, states, planning records, and Markdown</span></header>

  <section aria-labelledby="controls"><h2 id="controls">Controls</h2>
    <div class="row"><Button variant="primary" label="Create capture" /><Button label="Save changes" /><Button variant="ghost" label="Cancel" /><Button variant="danger" label="Discard" /></div>
    <div class="row"><Button label="Unavailable" disabled /><Button label="Saving changes" busy /><IconButton name="capture" label="Add capture" /><IconButton name="task" label="Show tasks" selected /><Menu items={menuItems} /></div>
    <div class="fields"><Field label="Title" bind:value={title} description="Use a short, actionable name." /><Select label="Priority" value="high" options={[{ value: "low", label: "Low" }, { value: "high", label: "High" }]} /></div>
    <Field label="Project path" value="/missing/project" error="Choose a directory that contains an Arc Lightning project." />
  </section>

  <section aria-labelledby="records"><h2 id="records">Planning records</h2>
    <div class="row"><StatusBadge status="draft" /><StatusBadge status="ready" /><StatusBadge status="in_progress" /><StatusBadge status="complete" /><PriorityBadge priority="low" /><PriorityBadge priority="critical" /></div>
    <RecordSummary record={task} selected={selected === task.id} onselect={(record) => selected = record.id} />
    <PlanningTree nodes={tree} selectedId={selected} onselect={(node) => selected = node.id} />
    <Readiness ready={false} reasons={["Waiting for Export workspace atomically", "Plan is still in progress"]} />
  </section>

  <section aria-labelledby="feedback"><h2 id="feedback">Feedback and content</h2>
    <Tabs label="Work queues" items={tabs} />
    <Feedback title="Workspace and database diverged" message="Choose which side to keep before Arc Lightning writes another record." tone="danger" />
    <EmptyState title="Inbox clear" message="New captures will appear here for triage." />
    <Markdown source={`### ${title}\n\nKeep planning context close to executable work.\n\n- Compare the common base\n- Stop on **true divergence**`} />
  </section>
</div>

<style>
  .review { min-height: 100vh; padding: 2rem; color: var(--arcl-text); background: var(--arcl-canvas); }
  header { max-width: 72rem; margin: 0 auto 2rem; }
  header p, header span { margin: 0; color: var(--arcl-text-muted); font-family: var(--arcl-font-code); }
  h1 { margin: 0.25rem 0; color: var(--arcl-heading); font: 680 2.5rem/1.1 var(--arcl-font-ui); }
  .review > section { display: grid; max-width: 72rem; gap: var(--arcl-space-4); margin: 0 auto; padding: var(--arcl-space-5); border-top: 1px solid var(--arcl-border); }
  h2 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-xl)/1.2 var(--arcl-font-ui); }
  .row { display: flex; flex-wrap: wrap; gap: var(--arcl-space-2); align-items: center; }
  .fields { display: grid; grid-template-columns: minmax(0, 2fr) minmax(10rem, 1fr); gap: var(--arcl-space-4); }
  @media (max-width: 38rem) { .review { padding: 1rem; } .fields { grid-template-columns: 1fr; } }
</style>
