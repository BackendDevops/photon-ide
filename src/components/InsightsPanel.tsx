import { useEffect, useState } from "react";
import { api, type Insights } from "../lib/api";

// Repository insights dashboard (docs/16 §8): contributors, recent activity,
// and the most-churned files — computed in Rust from `git log`.
export default function InsightsPanel({ refreshKey }: { refreshKey: number }) {
  const [data, setData] = useState<Insights | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .gitInsights()
      .then((d) => {
        setData(d);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, [refreshKey]);

  if (error) return <div className="p-4 text-fg-faint text-xs">No history. {error}</div>;
  if (!data) return <div className="p-4 text-fg-faint text-xs">Loading insights…</div>;

  const maxAct = Math.max(1, ...data.activity.map(([, n]) => n));
  const maxContrib = Math.max(1, ...data.contributors.map(([, n]) => n));
  const maxFile = Math.max(1, ...data.files.map(([, n]) => n));

  const Card = ({ title, children }: { title: string; children: React.ReactNode }) => (
    <div className="rounded-lg border border-border bg-bg-panel p-3">
      <div className="text-xs text-fg-muted mb-2 font-medium">{title}</div>
      {children}
    </div>
  );

  return (
    <div className="flex-1 overflow-auto p-4 space-y-4">
      <div className="flex items-baseline gap-2">
        <span className="text-2xl font-semibold text-fg">{data.total_commits.toLocaleString()}</span>
        <span className="text-fg-faint text-sm">commits total</span>
      </div>

      <Card title="Activity (last 14 active days)">
        <div className="flex items-end gap-1 h-24">
          {data.activity.map(([date, n]) => (
            <div key={date} className="flex-1 flex flex-col items-center justify-end group" title={`${date}: ${n} commit(s)`}>
              <div
                className="w-full rounded-t bg-accent/70 group-hover:bg-accent transition-colors"
                style={{ height: `${(n / maxAct) * 100}%`, minHeight: 2 }}
              />
              <span className="text-[8px] text-fg-faint mt-1 rotate-45 origin-left whitespace-nowrap">
                {date.slice(5)}
              </span>
            </div>
          ))}
          {data.activity.length === 0 && <span className="text-fg-faint text-xs">No recent activity.</span>}
        </div>
      </Card>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <Card title="Top contributors">
          <div className="space-y-1.5">
            {data.contributors.map(([name, n]) => (
              <div key={name} className="flex items-center gap-2 text-xs">
                <span className="w-28 truncate text-fg" title={name}>{name}</span>
                <div className="flex-1 h-2 rounded bg-bg-elevated overflow-hidden">
                  <div className="h-full bg-[#3fb950]" style={{ width: `${(n / maxContrib) * 100}%` }} />
                </div>
                <span className="w-8 text-right text-fg-faint tabular-nums">{n}</span>
              </div>
            ))}
          </div>
        </Card>

        <Card title="Most-changed files (recent)">
          <div className="space-y-1.5">
            {data.files.map(([path, n]) => (
              <div key={path} className="flex items-center gap-2 text-xs">
                <span className="flex-1 truncate text-fg font-mono" title={path}>
                  {path.split("/").pop()}
                </span>
                <div className="w-20 h-2 rounded bg-bg-elevated overflow-hidden">
                  <div className="h-full bg-[#d29922]" style={{ width: `${(n / maxFile) * 100}%` }} />
                </div>
                <span className="w-8 text-right text-fg-faint tabular-nums">{n}</span>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}
