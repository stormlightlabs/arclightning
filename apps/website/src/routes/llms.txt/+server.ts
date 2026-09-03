import { docs } from "$lib/content";

export const prerender = true;

export function GET(): Response {
  const pages = docs.map((doc) => `- [${doc.title}](/${doc.slug}.md): ${doc.description}`);
  const body = [
    "# Arc Lightning",
    "",
    "> Local-first project planning and execution for developers and software agents.",
    "",
    "Arc Lightning connects captures, owned specifications, persistent plans, tasks, notes, handoffs, and evidence in one project model.",
    "",
    "## Documentation",
    "",
    ...pages,
    "",
  ].join("\n");
  return new Response(body, { headers: { "content-type": "text/plain; charset=utf-8" } });
}
