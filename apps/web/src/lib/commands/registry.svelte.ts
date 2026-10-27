import { untrack } from "svelte";
import type { Command, CommandContext } from "./menu";

/** What one view or component offers the command menu and the shortcuts. */
export type Registration = {
  commands: Command[];
  /** The record the registration's selection commands act on. */
  context?: CommandContext | null;
};

/** The registrations in the order they were made, by owner. */
let registrations = $state<{ owner: symbol; registration: Registration }[]>([]);

/**
 * Offer the commands `build` returns for as long as the calling component is
 * mounted. `build` reruns when the state it reads changes, so the commands
 * follow the view's selection and status. Call it during component setup.
 */
export function useCommands(build: () => Registration): void {
  const owner = Symbol("commands");
  $effect(() => {
    const registration = build();
    untrack(() => {
      const index = registrations.findIndex((entry) => entry.owner === owner);
      if (index === -1) registrations.push({ owner, registration });
      else registrations[index] = { owner, registration };
    });
  });
  $effect(() => () => {
    registrations = registrations.filter((entry) => entry.owner !== owner);
  });
}

/** Every registered command, in registration order. */
export function registeredCommands(): Command[] {
  return registrations.flatMap((entry) => entry.registration.commands);
}

/** The latest registration's selected record, or `null` when none has one. */
export function registeredContext(): CommandContext | null {
  for (let index = registrations.length - 1; index >= 0; index--) {
    const context = registrations[index].registration.context;
    if (context) return context;
  }
  return null;
}
