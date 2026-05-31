import { useEffect, useState } from "react";
import {
  api,
  type Binding,
  type EventListener,
  type JobInfo,
  type ModelInfo,
  type MissingTranslation,
} from "../lib/api";

const REL_COLOR: Record<string, string> = {
  hasOne: "#5b8cff",
  hasMany: "#3fd07e",
  belongsTo: "#f0b429",
  belongsToMany: "#b07bff",
  morphTo: "#36d6c3",
  morphMany: "#36d6c3",
};

type Tab = "models" | "bindings" | "events" | "jobs" | "i18n";

// Laravel intelligence panel — models/relations, container bindings,
// event→listener wiring, queued jobs, and missing translations. (docs/06)
export default function ModelsPanel({
  onPick,
}: {
  onPick: (file: string, line: number) => void;
}) {
  const [tab, setTab] = useState<Tab>("models");
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [bindings, setBindings] = useState<Binding[]>([]);
  const [events, setEvents] = useState<EventListener[]>([]);
  const [jobs, setJobs] = useState<JobInfo[]>([]);
  const [missing, setMissing] = useState<MissingTranslation[]>([]);

  useEffect(() => {
    api.listModels().then(setModels).catch(() => {});
    api.listBindings().then(setBindings).catch(() => {});
    api.listEvents().then(setEvents).catch(() => {});
    api.listJobs().then(setJobs).catch(() => {});
    api.missingTranslations().then(setMissing).catch(() => {});
  }, []);

  const TabBtn = ({ id, label, n }: { id: Tab; label: string; n: number }) => (
    <button
      onClick={() => setTab(id)}
      className={`px-2 py-1.5 text-2xs whitespace-nowrap ${
        tab === id ? "text-accent border-b-2 border-accent" : "text-fg-faint"
      }`}
    >
      {label}
      {n > 0 ? ` ${n}` : ""}
    </button>
  );

  return (
    <div className="h-full flex flex-col text-sm">
      <div className="flex border-b border-line overflow-x-auto">
        <TabBtn id="models" label="Models" n={models.length} />
        <TabBtn id="bindings" label="Bindings" n={bindings.length} />
        <TabBtn id="events" label="Events" n={events.length} />
        <TabBtn id="jobs" label="Jobs" n={jobs.length} />
        <TabBtn id="i18n" label="i18n" n={missing.length} />
      </div>

      <div className="flex-1 overflow-y-auto pb-4">
        {tab === "models" &&
          (models.length === 0 ? (
            <Empty text="No Eloquent models found." />
          ) : (
            models.map((m) => (
              <div key={m.file} className="py-1">
                <div className="row" onClick={() => onPick(m.file, m.line)}>
                  <Badge letter="M" color="#e5a13a" />
                  <span className="text-fg">{m.name}</span>
                  {m.table && <span className="text-fg-faint text-xs">→ {m.table}</span>}
                </div>
                {m.relations.map((r, i) => (
                  <div key={i} className="row pl-7" onClick={() => onPick(m.file, r.line)}>
                    <span className="w-2 h-2 rounded-full" style={{ background: REL_COLOR[r.rel_type] || "#9aa4b2" }} />
                    <span className="text-fg-muted">{r.method}</span>
                    <span className="text-fg-faint text-xs">
                      {r.rel_type}
                      {r.related ? ` · ${r.related}` : ""}
                    </span>
                  </div>
                ))}
              </div>
            ))
          ))}

        {tab === "bindings" &&
          (bindings.length === 0 ? (
            <Empty text="No container bindings found." />
          ) : (
            bindings.map((b, i) => (
              <div key={i} className="row" onClick={() => onPick(b.file, b.line)}>
                <Badge letter="B" color="#36d6c3" />
                <span className="text-fg truncate">{b.abstract_name}</span>
                <span className="text-fg-faint text-xs ml-auto">{b.kind}</span>
                {b.concrete && <span className="text-fg-faint text-xs">→ {b.concrete}</span>}
              </div>
            ))
          ))}

        {tab === "events" &&
          (events.length === 0 ? (
            <Empty text="No event listeners found." />
          ) : (
            events.map((e, i) => (
              <div key={i} className="row" onClick={() => onPick(e.file, e.line)}>
                <Badge letter="E" color="#b07bff" />
                <span className="text-fg-muted truncate">{e.event}</span>
                <span className="text-fg-faint">→</span>
                <span className="text-fg-faint text-xs truncate">{e.listener}</span>
              </div>
            ))
          ))}

        {tab === "jobs" &&
          (jobs.length === 0 ? (
            <Empty text="No jobs found." />
          ) : (
            jobs.map((j, i) => (
              <div key={i} className="row" onClick={() => onPick(j.file, j.line)}>
                <Badge letter="J" color="#5b8cff" />
                <span className="text-fg truncate">{j.name}</span>
                {j.queued && <span className="chip bg-running/15 text-running ml-auto">queued</span>}
              </div>
            ))
          ))}

        {tab === "i18n" &&
          (missing.length === 0 ? (
            <Empty text="No missing translations. 🎉" />
          ) : (
            missing.map((m) => (
              <div key={m.key} className="px-3 py-1.5 border-b border-line/40">
                <div className="text-fg truncate">{m.key}</div>
                <div className="text-xs">
                  <span className="text-success">{m.present_in.join(", ") || "—"}</span>
                  <span className="text-fg-faint"> · missing: </span>
                  <span className="text-danger">{m.missing_in.join(", ")}</span>
                </div>
              </div>
            ))
          ))}
      </div>
    </div>
  );
}

function Empty({ text }: { text: string }) {
  return <div className="px-3 py-4 text-fg-faint text-xs">{text}</div>;
}
function Badge({ letter, color }: { letter: string; color: string }) {
  return (
    <span
      className="inline-flex items-center justify-center w-[16px] h-[16px] rounded text-2xs font-bold shrink-0"
      style={{ color, background: `${color}22` }}
    >
      {letter}
    </span>
  );
}
