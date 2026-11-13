// One copy-to-clipboard helper for the inspector and location lists: it remembers the copied
// text for a moment (for a "Copied" label) and clears its timer when copied again or disposed.
export interface CopyState {
  readonly copied: string | null;
  copy(text: string): Promise<boolean>;
  dispose(): void;
}

export function createCopy(holdMs = 1600, clipboard: () => Clipboard | undefined = () => globalThis.navigator?.clipboard): CopyState {
  let copied = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    get copied() { return copied; },
    async copy(text: string) {
      clearTimeout(timer);
      try {
        const target = clipboard();
        if (!target) throw new Error("Clipboard unavailable");
        await target.writeText(text);
        copied = text;
        timer = setTimeout(() => { copied = null; }, holdMs);
        return true;
      } catch {
        copied = null;
        return false;
      }
    },
    dispose() { clearTimeout(timer); },
  };
}
