/** Iconify classes selected for Arc Lightning product concepts. */
export const ICONS = {
  capture: "i-ri-add-line",
  close: "i-ri-close-line",
  more: "i-ri-more-2-fill",
  note: "i-ri-sticky-note-line",
  plan: "i-ri-git-branch-line",
  spec: "i-ri-file-text-line",
  task: "i-ri-checkbox-circle-line",
} as const;

/** A semantic icon name supported by the shared icon component. */
export type IconName = keyof typeof ICONS;
