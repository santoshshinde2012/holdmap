// Shared skill-trust state for Security, Home and the agent views: loaded once, refreshed after a
// scan, a trust change or a scheduled run. Reads stored records; `scan()` fingerprints again.
import * as api from "./api";
import { alerts, type SkillTrustView, type TrustReport } from "./trust";

class TrustStore {
  report = $state<TrustReport | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);
  private loading: Promise<void> | null = null;

  /** Skills that need a look: changed since trusted, or never trusted. */
  readonly alerting = $derived(this.report?.skills.filter((s) => alerts(s.state)) ?? []);

  load(): Promise<void> {
    this.loading ??= this.guard(async () => { this.report = await api.skillTrust(); }).finally(() => (this.loading = null));
    return this.loading;
  }
  scan(): Promise<void> { return this.guard(async () => { this.report = await api.scanSkillTrust(); }); }
  async set(id: string, trusted: boolean, reason?: string): Promise<SkillTrustView | null> {
    let view: SkillTrustView | null = null;
    await this.guard(async () => { view = await api.setSkillTrust(id, trusted, reason); this.report = await api.skillTrust(); });
    return view;
  }
  /** Alerting skills that belong to one agent product (subject ids start with `product:`). */
  forAgent(product: string): SkillTrustView[] { return this.alerting.filter((s) => s.subject.id.startsWith(`${product}:`)); }

  private async guard(fn: () => Promise<void>) {
    this.busy = true; this.error = null;
    try { await fn(); } catch (e) { this.error = e instanceof Error ? e.message : String(e); } finally { this.busy = false; }
  }
}

export const trustStore = new TrustStore();
