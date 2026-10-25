/** Crockford's base32 alphabet, which ULIDs use. */
const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** How long each relative unit lasts, in ms, from the largest. */
const UNITS = [
  { suffix: "d", ms: 86_400_000 },
  { suffix: "h", ms: 3_600_000 },
  { suffix: "m", ms: 60_000 },
] as const;

/** Relative labels stop at this age, in ms; older records show their date. */
const RELATIVE_LIMIT = 7 * 86_400_000;

/**
 * When a record was created, read from the ULID that ends its ID, such as
 * `arcl-c-01M5QSTZR7D5Y37H635KD8NA44`. A ULID's first ten characters are the
 * time arcl minted it, in ms, and arcl mints the ID as it creates the record,
 * so the captures listing needs no timestamp field. `null` when the ID ends
 * in no ULID.
 */
export function createdAt(id: string): Date | null {
  const match = /-([0-7][0-9A-HJKMNP-TV-Z]{25})$/i.exec(id);
  if (match === null) return null;
  let ms = 0;
  for (const char of match[1].slice(0, 10).toUpperCase()) ms = ms * 32 + CROCKFORD.indexOf(char);
  return new Date(ms);
}

/**
 * `date` for a table cell: `just now`, then `5m ago`, `3h ago` or `2d ago`
 * for a week, then the date, such as `Oct 20`, with the year once it is not
 * `now`'s.
 */
export function shortAge(date: Date, now: Date): string {
  const age = now.getTime() - date.getTime();
  if (age >= 0 && age < RELATIVE_LIMIT) {
    const unit = UNITS.find((candidate) => age >= candidate.ms);
    return unit ? `${Math.floor(age / unit.ms)}${unit.suffix} ago` : "just now";
  }
  const year = date.getFullYear() === now.getFullYear() ? undefined : "numeric";
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric", year });
}
