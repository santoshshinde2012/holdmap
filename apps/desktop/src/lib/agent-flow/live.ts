/** A view owns one request generation; pause, navigation and disposal invalidate it. */
export function createFlowLoader<T>(
  pull: () => Promise<T>,
  apply: (value: T) => void,
  fail: (message: string | null) => void,
  loading: (busy: boolean) => void,
) {
  let generation = 0;
  let pending: Promise<void> | null = null;
  function invalidate() {
    generation++;
    pending = null;
    loading(false);
  }
  function request(): Promise<void> {
    if (pending) return pending;
    const current = ++generation;
    loading(true);
    const task = Promise.resolve().then(pull).then((value) => {
      if (current !== generation) return;
      apply(value);
      fail(null);
    }).catch((error: unknown) => {
      if (current === generation) fail(error instanceof Error ? error.message : String(error));
    }).finally(() => {
      if (current === generation) {
        pending = null;
        loading(false);
      }
    });
    pending = task;
    return task;
  }
  return { request, invalidate };
}
