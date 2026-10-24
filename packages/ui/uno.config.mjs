import presetIcons from "@unocss/preset-icons";
import { defineConfig } from "unocss";

/**
 * Arc Lightning's UnoCSS config, shared by the UI package and the apps.
 *
 * The icon collections load from this package, so an app that uses the
 * config does not need its own `@iconify-json` dependencies: Phosphor (`ph`),
 * Bootstrap (`bi`) and Remix (`ri`).
 */
export default defineConfig({
  presets: [
    presetIcons({
      collections: {
        bi: () => import("@iconify-json/bi").then((collection) => collection.icons),
        ph: () => import("@iconify-json/ph").then((collection) => collection.icons),
        ri: () => import("@iconify-json/ri").then((collection) => collection.icons),
      },
      extraProperties: {
        display: "inline-block",
        "flex-shrink": "0",
        "vertical-align": "-0.125em",
      },
    }),
  ],
});
