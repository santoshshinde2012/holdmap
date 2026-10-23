/**
 * Privacy rule for screen capture: Holdmap records only its own window, never another app or a whole screen.
 * The browser decides what the picker offers, so every stream is checked here before a single frame is read.
 */

export type SurfaceVerdict = { ok: true; surface: "browser" | "window" } | { ok: false; message: string };

export const OWN_WINDOW_ONLY = "Records only the holdmap window";

const REFUSED = "Nothing was recorded.";
const messages = {
  monitor: `That shares a whole screen, and Holdmap records only its own window. ${REFUSED} Choose the holdmap tab or window and start again.`,
  other: `That shares a different window, and Holdmap records only its own window. ${REFUSED} Choose the holdmap tab or window and start again.`,
  unknown: `This browser can't confirm that only the holdmap window is shared, so ${REFUSED.toLowerCase()} Use the holdmap desktop app, or a Chromium browser that can share this tab.`,
  none: `No picture was shared. ${REFUSED} Choose the holdmap tab or window and start again.`,
} as const;

type SurfaceTrack = Pick<MediaStreamTrack, "label" | "getSettings">;

/** Accepts a tab, or a window whose title is Holdmap's own. Refuses a screen, another window, or a source it can't identify. */
export function checkOwnSurface(track: SurfaceTrack | undefined, ownNames: readonly string[]): SurfaceVerdict {
  if (!track) return { ok: false, message: messages.none };
  const surface = (track.getSettings() as { displaySurface?: string }).displaySurface;
  if (surface === "browser") return { ok: true, surface };
  if (surface === "monitor") return { ok: false, message: messages.monitor };
  if (surface === "window") {
    const label = track.label.trim().toLowerCase();
    const mine = ownNames.map((name) => name.trim().toLowerCase()).filter(Boolean);
    return label && mine.some((name) => label === name || label.startsWith(`${name} `) || label.startsWith(`${name} -`) || label.startsWith(`${name} \u2013`))
      ? { ok: true, surface }
      : { ok: false, message: messages.other };
  }
  return { ok: false, message: messages.unknown };
}

/** getDisplayMedia hints that steer the picker to this tab or window. Hints only: `checkOwnSurface` is the enforcement. */
export function ownWindowDisplayOptions(resolution: { width: number; height: number }): DisplayMediaStreamOptions {
  return {
    video: { displaySurface: "browser", frameRate: 30, width: { max: resolution.width }, height: { max: resolution.height } },
    audio: false,
    preferCurrentTab: true,
    selfBrowserSurface: "include",
    surfaceSwitching: "exclude",
    monitorTypeSurfaces: "exclude",
    systemAudio: "exclude",
  } as DisplayMediaStreamOptions;
}

/** Stops a stream that shows more than the holdmap window and returns the reason, or null when it is fine. */
export function refuseWiderSurface(stream: MediaStream, ownNames: readonly string[]): string | null {
  const tracks = stream.getVideoTracks();
  const verdicts = tracks.length ? tracks.map((track) => checkOwnSurface(track, ownNames)) : [checkOwnSurface(undefined, ownNames)];
  const refused = verdicts.find((verdict) => !verdict.ok);
  if (!refused || refused.ok) return null;
  for (const track of stream.getTracks()) track.stop();
  return refused.message;
}
