// One way to write a count with its noun, so the UI never says "1 Services" or "1 links".
export function noun(n: number, one: string, many = `${one}s`): string {
  return n === 1 ? one : many;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${noun(n, one, many)}`;
}
