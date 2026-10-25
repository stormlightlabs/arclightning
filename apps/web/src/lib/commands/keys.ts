import { inField } from "$lib/work/keys";

/** How long the second key of a sequence such as `g` `w` may wait, in milliseconds. */
export const SEQUENCE_TIMEOUT = 1000;

/** The parts of a keyboard event the shortcut rules read. */
export interface ShortcutKey {
  key: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  defaultPrevented: boolean;
  target: EventTarget | null;
}

/** A command a shortcut can run: the keys pressed one after another. */
export interface Bound {
  keys: string[];
}

/** What a key press does to a shortcut sequence in progress. */
export interface ShortcutStep<T> {
  /** The command the sequence completed, if any. */
  match: T | null;
  /** The keys pressed so far toward a longer shortcut; empty when none is pending. */
  pending: string[];
}

/** Whether the browser runs on macOS, where `⌘` stands in for `Ctrl`. */
export function isMac(platform: string): boolean {
  return /mac|iphone|ipad/i.test(platform);
}

/**
 * Whether a key press opens the command menu: `⌘K` on macOS and `Ctrl+K`
 * elsewhere, outside form fields and the editor.
 */
export function opensMenu(event: ShortcutKey, mac: boolean): boolean {
  const modifier = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  return modifier && !event.altKey && event.key.toLowerCase() === "k" && !event.defaultPrevented && !inField(event.target);
}

/**
 * Whether single-key shortcuts may act on a key press. Keys typed into a form
 * field, the editor or a dialog, held with `Ctrl`, `⌘` or `Alt`, or already
 * handled are left alone.
 */
export function takesShortcut(event: ShortcutKey): boolean {
  return !(event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || inField(event.target));
}

/**
 * Advance a shortcut sequence by `key`. A command whose keys equal the
 * pressed sequence matches; a sequence that starts a longer shortcut stays
 * pending; anything else starts over from `key` alone.
 */
export function shortcutStep<T extends Bound>(pending: string[], key: string, commands: T[]): ShortcutStep<T> {
  const attempt = (sequence: string[]): ShortcutStep<T> | null => {
    const match = commands.find((command) => sameKeys(command.keys, sequence));
    if (match) return { match, pending: [] };
    const longer = commands.some((command) => command.keys.length > sequence.length && sameKeys(command.keys.slice(0, sequence.length), sequence));
    return longer ? { match: null, pending: sequence } : null;
  };
  return (pending.length ? attempt([...pending, key]) : null) ?? attempt([key]) ?? { match: null, pending: [] };
}

/** `keys` as the menu shows them, with `Mod` as `⌘` on macOS and `Ctrl` elsewhere. */
export function formatKeys(keys: string[], mac: boolean): string[] {
  const names: Record<string, string> = { Mod: mac ? "⌘" : "Ctrl", Enter: "↵", Escape: "Esc", Shift: "⇧" };
  return keys.map((key) => names[key] ?? (key.length === 1 ? key.toUpperCase() : key));
}

function sameKeys(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((key, index) => key === right[index]);
}
