// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

// https://astro.build/config
export default defineConfig({
  integrations: [
    starlight({
      title: "Arc Lightning",
      description: "A Git-aware task tracker for developers and coding agents.",
      customCss: [
        "@fontsource-variable/ibm-plex-sans",
        "@fontsource-variable/literata",
        "./src/styles/theme.css",
      ],
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/stormlightlabs/arclightning" }],
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "Overview", slug: "overview" },
            { label: "Quick Start", slug: "quick-start" },
          ],
        },
        {
          label: "Guides",
          items: [{ autogenerate: { directory: "guides" } }],
        },
        {
          label: "Reference",
          items: [{ autogenerate: { directory: "reference" } }],
        },
      ],
    }),
  ],
});
