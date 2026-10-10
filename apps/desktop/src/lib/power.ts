/** A native, single-use preview. Browser demos use the same shape for a simulation. */
export interface ShutdownPreview {
  confirmation_id: string;
  platform: string;
  hostname: string;
  expires_in_secs: number;
}

/** Invalid or expired previews fail closed; the UI never extends a token beyond one minute. */
export function shutdownPreviewLifetimeMs(preview: ShutdownPreview | null): number {
  if (!preview?.confirmation_id || !Number.isFinite(preview.expires_in_secs) || preview.expires_in_secs <= 0) return 0;
  return Math.min(preview.expires_in_secs, 60) * 1000;
}
