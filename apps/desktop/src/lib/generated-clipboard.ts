/** Copy generated text without losing the click's clipboard permission in WebKit. */
export async function copyGeneratedText(generate: () => Promise<string>, isCurrent: () => boolean = () => true): Promise<void> {
  try {
    const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
    if (!clipboard || !isCurrent()) throw new Error("Clipboard unavailable");

    if (typeof clipboard.write === "function" && typeof ClipboardItem !== "undefined") {
      // Start the clipboard operation during the click; native configuration arrives later.
      const data = Promise.resolve().then(() => {
        if (!isCurrent()) throw new Error("Clipboard unavailable");
        return generate();
      }).then((text) => {
        if (!isCurrent()) throw new Error("Clipboard unavailable");
        return new Blob([text], { type: "text/plain" });
      });
      // A constructor or early clipboard rejection can leave the data promise unconsumed.
      void data.catch(() => undefined);
      const writing = clipboard.write([new ClipboardItem({ "text/plain": data })]);
      await Promise.all([writing, data]);
    } else {
      if (typeof clipboard.writeText !== "function") throw new Error("Clipboard unavailable");
      const text = await generate();
      if (!isCurrent()) throw new Error("Clipboard unavailable");
      await clipboard.writeText(text);
    }
  } catch {
    // Native configuration and clipboard errors can contain private setup credentials.
    throw new Error("Clipboard unavailable");
  }
}
