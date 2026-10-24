/** What a key does in the Work list. */
export type WorkKeyAction = "next" | "previous" | "open";

/** The parts of a keyboard event {@link workKeyAction} reads. */
export interface WorkKey {
  key: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  target: EventTarget | null;
}

/**
 * The Work list action for a key press, or `null` when the key is not the
 * list's: `j` and `ArrowDown` select the next row, `k` and `ArrowUp` the
 * previous one, and `Enter` opens the selected task. Keys typed into a form
 * field or held with a modifier are left alone, and so is `Enter` on a link
 * or button, which opens what it names itself.
 */
export function workKeyAction(event: WorkKey): WorkKeyAction | null {
  if (event.altKey || event.ctrlKey || event.metaKey) return null;
  const element = closestOf(event.target);
  if (element?.closest("input, textarea, select, [contenteditable]:not([contenteditable='false'])")) return null;
  switch (event.key) {
    case "j":
    case "ArrowDown":
      return "next";
    case "k":
    case "ArrowUp":
      return "previous";
    case "Enter":
      return element?.closest("a, button") ? null : "open";
    default:
      return null;
  }
}

/**
 * The row to select after moving `step` rows from `current` through `order`.
 * With no selection, or one the list no longer shows, the first row comes
 * next and the last row comes before. Moving stops at either end.
 */
export function moveSelection(order: string[], current: string | null, step: 1 | -1): string | null {
  if (order.length === 0) return null;
  const index = current === null ? -1 : order.indexOf(current);
  if (index === -1) return step === 1 ? order[0] : order[order.length - 1];
  return order[Math.min(order.length - 1, Math.max(0, index + step))];
}

/** `target` as something with `closest`, which every element has. */
function closestOf(target: EventTarget | null): Pick<Element, "closest"> | null {
  const candidate = target as Partial<Pick<Element, "closest">> | null;
  return typeof candidate?.closest === "function" ? (candidate as Pick<Element, "closest">) : null;
}
