// Browser demo for skill trust and scheduled scans: labelled sample data, kept in memory.
// Nothing is read from disk, hashed for real, scheduled or notified.
import { demoSkillSubjects } from "./analyzers-demo";
import type { AnalyzerView } from "./analyzers";
import type { SkillTrustView, TrustReport } from "./trust";
import { DEFAULT_SCHEDULE, widens, type Due, type Schedule, type ScheduleFinished, type SchedulePlan, type ScheduleRequest, type ScheduleStatus } from "./schedule";

const DAY = 86_400_000;
const hex = (seed: string) => [...Array(64)].map((_, i) => "0123456789abcdef"[(seed.charCodeAt(i % seed.length) * (i + 7)) % 16]).join("");

export class DemoTrust {
  private skills: SkillTrustView[];
  private scanned: number | null;
  private schedule: Schedule = { ...DEFAULT_SCHEDULE };

  constructor(fresh = false) {
    const now = Date.now();
    const subjects = demoSkillSubjects();
    this.scanned = fresh ? null : now - 2 * 3_600_000;
    this.skills = fresh ? [] : subjects.map((subject, i): SkillTrustView => {
      const base = { subject, sha256: hex(subject.id), file_count: 3 + i, total_bytes: 4_200 + i * 1_900, modified_ms: now - (i + 1) * DAY, partial: false, first_seen_ms: now - 30 * DAY, seen_ms: now - 2 * 3_600_000, changed_since_last_scan: false };
      if (subject.name === "release-check") return { ...base, state: "changed", changed_since_last_scan: true, trusted_ms: now - 12 * DAY, reason: "Reviewed with the release team", changes: { added: [{ path: "scripts/upload.sh", size_after: 1_204 }], removed: [], modified: [{ path: "SKILL.md", size_before: 2_310, size_after: 2_988 }], added_total: 1, removed_total: 0, modified_total: 1 } };
      if (subject.name === "db-migrations") return { ...base, state: "new", first_seen_ms: now - 2 * 3_600_000 };
      return { ...base, state: "trusted", trusted_ms: now - 20 * DAY, reason: i === 0 ? "Our own skill" : null };
    });
  }

  report(): TrustReport {
    const order = { changed: 0, new: 1, unreadable: 2, trusted: 3 } as const;
    const skills = [...this.skills].sort((a, b) => order[a.state] - order[b.state] || a.subject.name.localeCompare(b.subject.name));
    return { skills, changed: skills.filter((s) => s.state === "changed").length, untrusted: skills.filter((s) => s.state === "new").length, scanned_ms: this.scanned };
  }
  scan(): TrustReport {
    this.scanned = Date.now();
    this.skills = this.skills.map((s) => ({ ...s, seen_ms: this.scanned!, changed_since_last_scan: false }));
    return this.report();
  }
  set(id: string, trusted: boolean, reason: string | null): SkillTrustView {
    const i = this.skills.findIndex((s) => s.subject.id === id);
    if (i < 0) throw new Error(`No skill ${id}. Scan skills first.`);
    const s = this.skills[i];
    this.skills[i] = trusted ? { ...s, state: "trusted", trusted_ms: Date.now(), reason, changes: null, changed_since_last_scan: false } : { ...s, state: "new", trusted_ms: null, reason: null, changes: null };
    return this.skills[i];
  }

  plan(views: AnalyzerView[], every_hours: number, mcp_opt_in: string[], skip_on_battery: boolean): SchedulePlan {
    return {
      every_hours, network: "none",
      analyzers: views.map((v) => ({ id: v.id, name: v.name, starts_mcp_server: v.kind === "mcp" && mcp_opt_in.includes(v.id), blocked: v.state !== "approved" ? "Turn it on first." : v.permissions.network.length ? "Uses the network, so it only runs when you start it." : v.kind === "mcp" && !mcp_opt_in.includes(v.id) ? "MCP analyzers run on a schedule only if you opt this one in." : null })),
      runs_when: `Only while holdmap is open${skip_on_battery ? ", not on battery" : ""}. holdmap has no background service; nothing runs after you quit.`,
      notifies: "New high or critical findings, and skills changed since you trusted them. At most once an hour, never in quiet hours.",
    };
  }
  scheduleStatus(views: AnalyzerView[]): ScheduleStatus {
    const s = this.schedule;
    const from = s.last_run_ms ?? s.confirmed_ms;
    return { schedule: s, plan: this.plan(views, s.every_hours, s.mcp_opt_in, s.skip_on_battery), next_ms: s.enabled && from ? from + s.every_hours * 3_600_000 : null, power: "unknown" };
  }
  setSchedule(views: AnalyzerView[], req: ScheduleRequest): ScheduleStatus {
    if (req.enabled) {
      const plan = this.plan(views, req.every_hours, req.mcp_opt_in, req.skip_on_battery);
      if (!req.analyzers.length) throw new Error("choose at least one analyzer that can run on a schedule");
      const bad = plan.analyzers.find((a) => req.analyzers.includes(a.id) && a.blocked);
      if (bad) throw new Error(`${bad.name}: ${bad.blocked}`);
      if (widens(this.schedule, req) && !req.confirmed) throw new Error("confirm what will run before turning on scheduled scans");
    }
    const { confirmed, ...rest } = req;
    this.schedule = { ...this.schedule, ...rest, confirmed_ms: confirmed ? Date.now() : this.schedule.confirmed_ms };
    return this.scheduleStatus(views);
  }
  due(views: AnalyzerView[]): Due {
    const st = this.scheduleStatus(views);
    if (st.next_ms == null) return { kind: "off" };
    if (Date.now() < st.next_ms) return { kind: "wait", next_ms: st.next_ms };
    return { kind: "run", analyzers: this.schedule.analyzers };
  }
  finished(startedMs: number): ScheduleFinished {
    this.schedule = { ...this.schedule, last_run_ms: startedMs };
    return { outcome: { new_high: 0, new_critical: 0, changed_skills: 0 }, notice: null };
  }
}
