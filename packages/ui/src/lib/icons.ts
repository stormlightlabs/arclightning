/** Iconify classes selected for Arc Lightning product concepts. */
export const ICONS = {
  arrowLeft: "i-ri-arrow-left-line",
  arrowRight: "i-ri-arrow-right-line",
  capture: "i-ri-add-line",
  close: "i-ri-close-line",
  github: "i-ri-github-fill",
  inbox: "i-ph-tray",
  markdown: "i-ri-markdown-line",
  menu: "i-ri-menu-line",
  moon: "i-ri-moon-line",
  more: "i-ri-more-2-fill",
  note: "i-ri-sticky-note-line",
  plan: "i-ri-git-branch-line",
  planning: "i-bi-diagram-3",
  search: "i-ri-search-line",
  spec: "i-ri-file-text-line",
  sun: "i-ri-sun-line",
  task: "i-ri-checkbox-circle-line",
  work: "i-ph-list-checks",
} as const;

/** A semantic icon name supported by the shared icon component. */
export type IconName = keyof typeof ICONS;
