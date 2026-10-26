/**
 * Keyboard rules for the select-only combobox in `SelectMenu`, kept apart
 * from the component so unit tests can drive them. They follow the WAI-ARIA
 * Authoring Practices select-only combobox pattern.
 */

/** How long a type-ahead query lasts after its last key, in milliseconds. */
export const TYPEAHEAD_TIMEOUT = 500;

/** The parts of a keyboard event {@link listboxKey} reads. */
export interface ListboxKey {
  key: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}

/** The combobox state a key acts on. */
export interface ListboxState {
  open: boolean;
  /** The option index the listbox highlights. */
  active: number;
  /** Whether a type-ahead query is in progress, which makes Space part of it. */
  searching: boolean;
}

/** One option's label and whether it can be chosen. */
export interface ListboxOption {
  label: string;
  disabled?: boolean;
}

/** What a key does: the next state, and whether to choose the active option. */
export interface ListboxStep {
  open: boolean;
  active: number;
  /** Choose the option at `active`, then close. */
  commit: boolean;
  /** A character to add to the type-ahead query. */
  type?: string;
  /** Whether the key's default action should be stopped. Tab keeps its own. */
  prevent: boolean;
}

/**
 * The step for a key press, or `null` when the key is not the combobox's.
 *
 * Closed: Down, Up, Enter and Space open on the active option; Home and End
 * open on the first and last; a character opens and starts type-ahead.
 * Open: Down and Up move one option and stop at the ends; Home, End, Page Up
 * and Page Down jump; Enter, Space and Alt+Up choose the active option; Tab
 * chooses it and lets focus move on; Escape closes without choosing.
 * Disabled options are skipped.
 */
export function listboxKey(event: ListboxKey, state: ListboxState, options: ListboxOption[]): ListboxStep | null {
  if (event.ctrlKey || event.metaKey) return null;
  const { open, active } = state;
  const step = (fields: Partial<ListboxStep>): ListboxStep => ({
    open,
    active,
    commit: false,
    prevent: true,
    ...fields,
  });
  const first = nextEnabled(options, -1, 1);
  const last = nextEnabled(options, options.length, -1);
  const printable = event.key.length === 1 && !event.altKey && (event.key !== " " || state.searching);

  if (!open) {
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
      case "Enter":
      case " ":
        if (printable) return step({ open: true, type: event.key });
        return step({ open: true });
      case "Home":
        return step({ open: true, active: first });
      case "End":
        return step({ open: true, active: last });
      default:
        return printable ? step({ open: true, type: event.key }) : null;
    }
  }

  switch (event.key) {
    case "ArrowDown":
      return step({ active: nextEnabled(options, active, 1, active) });
    case "ArrowUp":
      if (event.altKey) return step({ open: false, commit: true });
      return step({ active: nextEnabled(options, active, -1, active) });
    case "Home":
      return step({ active: first });
    case "End":
      return step({ active: last });
    case "PageDown":
      return step({ active: jump(options, active, 10) });
    case "PageUp":
      return step({ active: jump(options, active, -10) });
    case "Enter":
      return step({ open: false, commit: true });
    case " ":
      return printable ? step({ type: " " }) : step({ open: false, commit: true });
    case "Escape":
      return step({ open: false });
    case "Tab":
      return step({ open: false, commit: true, prevent: false });
    default:
      return printable ? step({ type: event.key }) : null;
  }
}

/**
 * The option a type-ahead `query` lands on, or `null` when none matches.
 *
 * The search starts after `active` and wraps, so repeating one letter cycles
 * through the options that start with it. A longer query keeps the active
 * option while it still matches. Matching ignores case.
 */
export function typeahead(options: ListboxOption[], active: number, query: string): number | null {
  const needle = query.toLowerCase();
  const repeated = needle.length > 1 && [...needle].every((char) => char === needle[0]);
  const search = repeated ? needle[0] : needle;
  const start = needle.length > 1 && !repeated ? active : active + 1;
  for (let offset = 0; offset < options.length; offset++) {
    const index = (((start + offset) % options.length) + options.length) % options.length;
    const option = options[index];
    if (!option.disabled && option.label.toLowerCase().startsWith(search)) return index;
  }
  return null;
}

/** The first enabled option after `from` going `step`, or `fallback` when there is none. */
function nextEnabled(options: ListboxOption[], from: number, step: 1 | -1, fallback = from): number {
  for (let index = from + step; index >= 0 && index < options.length; index += step) {
    if (!options[index].disabled) return index;
  }
  return Math.min(Math.max(fallback, 0), Math.max(options.length - 1, 0));
}

/** The enabled option `distance` options from `active`, stopping at the ends and backing off disabled ones. */
function jump(options: ListboxOption[], active: number, distance: number): number {
  const target = Math.min(options.length - 1, Math.max(0, active + distance));
  if (!options[target]?.disabled) return target;
  return nextEnabled(options, target, distance > 0 ? -1 : 1, active);
}
