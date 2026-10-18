/**
 * Browser notices that arrive as `error` events but aren't failures. "ResizeObserver loop"
 * means some resize notifications waited for the next frame (Svelte Flow and layout switches
 * do this when the window narrows); remounting the app for it would lose the user's place.
 */
export function isBenignError(message: string): boolean {
  return /^ResizeObserver loop (completed with undelivered notifications|limit exceeded)/.test(message);
}
