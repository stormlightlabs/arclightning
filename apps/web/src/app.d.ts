import type { ChangeWarning } from "$lib/data";

declare global {
  namespace App {
    /** History state a navigation carries to the page it opens. */
    interface PageState {
      /** The warnings of the change that led here, such as a task create. */
      warnings?: ChangeWarning[];
    }
  }
}

export {};
