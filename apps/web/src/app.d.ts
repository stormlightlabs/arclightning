import type { ArtifactHost, ChangeWarning } from "$lib/data";

declare global {
  namespace App {
    /** History state a navigation carries to the page it opens. */
    interface PageState {
      /** The warnings of the change that led here, such as a task create. */
      warnings?: ChangeWarning[];
    }
  }

  interface ImportMetaEnv {
    /** `artifact` in the claude.ai artifact build; unset in the `arcl serve` build. */
    readonly VITE_ARCL_TARGET?: string;
  }

  interface Window {
    /** The claude.ai artifact host's capabilities; absent outside claude.ai. */
    claude?: ArtifactHost;
  }
}

export {};
