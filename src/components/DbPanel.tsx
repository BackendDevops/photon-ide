import { useCallback, useEffect, useState } from "react";
import { api, type DataSource, type DbSchema, type QueryResult } from "../lib/api";
import RedisConsole from "./RedisConsole";

const DRIVER_GLYPH: Record<string, string> = {
  mysql: "🐬",
  mariadb: "🦭",
  postgres: "🐘",
  sqlite: "📦",
};

// Data-source explorer: lists saved connection profiles, connects, and browses
// schema. Editing/creating profiles happens in DbConnectionDialog (via App).
export default function DbPanel({
  refreshKey,
  onNewSource,
  onEditSource,
  onRunQuery,
}: {
  refreshKey: number;
  onNewSource: () => void;
  onEditSource: (ds: DataSource) => void;
  onRunQuery: (connection: string, sql: string) => void;
}) {
  const [sources, setSources] = useState<DataSource[]>([]);
  const [schemas, setSchemas] = useState<Record<string, DbSchema>>({});
  const [openTables, setOpenTables] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [redisOpen, setRedisOpen] = useState(false);

  const load = useCallback(() => {
    api.dbListSources().then(setSources).catch(() => setSources([]));
  }, []);
  useEffect(load, [load, refreshKey]);

  const connect = async (ds: DataSource) => {
    setConnecting(ds.id);
    setError(null);
    try {
      await api.dbConnectSource(ds.id);
      const schema = await api.dbSchema(ds.name);
      setSchemas((s) => ({ ...s, [ds.name]: schema }));
    } catch (e) {
      setError(String(e));
    } finally {
      setConnecting(null);
    }
  };

  const toggle = (key: string) =>
    setOpenTables((s) => {
      const n = new Set(s);
      n.has(key) ? n.delete(key) : n.add(key);
      return n;
    });

  return (
    <div className="h-full flex flex-col text-sm">
      <div className="flex items-center justify-between px-2 py-1.5 border-b border-border">
        <span className="text-fg-faint text-xs">Data Sources</span>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setRedisOpen(true)}
            className="text-fg-faint hover:text-fg text-2xs px-1.5 py-0.5 rounded border border-border hover:bg-bg-hover"
            title="Open Redis console"
          >
            Redis
          </button>
          <button
            onClick={onNewSource}
            className="text-fg-faint hover:text-fg text-base leading-none"
            title="New data source"
          >
            +
          </button>
        </div>
      </div>
      {redisOpen && <RedisConsole onClose={() => setRedisOpen(false)} />}

      <div className="flex-1 overflow-y-auto pb-4">
        {error && <div className="px-3 py-2 text-danger text-xs">{error}</div>}
        {sources.length === 0 && (
          <div className="px-3 py-4 text-fg-faint text-xs leading-relaxed">
            No data sources yet.
            <button onClick={onNewSource} className="block mt-2 text-accent hover:underline">
              + New data source
            </button>
          </div>
        )}
        {sources.map((ds) => {
          const schema = schemas[ds.name];
          return (
            <div key={ds.id}>
              <div className="row group">
                <span>{DRIVER_GLYPH[ds.driver] || "🗄"}</span>
                <span className="truncate flex-1" title={`${ds.driver} · ${ds.host}`}>
                  {ds.name}
                </span>
                <span className="flex items-center gap-1 opacity-0 group-hover:opacity-100">
                  <button
                    onClick={() => connect(ds)}
                    className="text-fg-faint hover:text-accent text-xs"
                    title="Connect"
                  >
                    {connecting === ds.id ? "…" : "▷"}
                  </button>
                  <button
                    onClick={() => onEditSource(ds)}
                    className="text-fg-faint hover:text-fg text-xs"
                    title="Edit"
                  >
                    ✎
                  </button>
                </span>
              </div>
              {schema &&
                schema.tables.map((t) => {
                  const key = `${ds.name}.${t.name}`;
                  return (
                    <div key={key}>
                      <div className="row pl-6" onClick={() => toggle(key)}>
                        <span className="text-fg-faint text-[10px] w-3">
                          {openTables.has(key) ? "▾" : "▸"}
                        </span>
                        <span className="text-[#56b6c2]">▦</span>
                        <span className="truncate flex-1">{t.name}</span>
                        <span
                          className="text-fg-faint text-[10px] opacity-0 hover:opacity-100"
                          onClick={(e) => {
                            e.stopPropagation();
                            onRunQuery(ds.name, `SELECT * FROM ${t.name} LIMIT 100;`);
                          }}
                          title="Query"
                        >
                          ▷
                        </span>
                      </div>
                      {openTables.has(key) &&
                        t.columns.map((c) => (
                          <div key={c.name} className="row pl-10 text-xs">
                            <span className="text-fg-muted">{c.name}</span>
                            <span className="text-fg-faint">{c.data_type}</span>
                            {!c.nullable && <span className="text-danger text-[9px]">NN</span>}
                          </div>
                        ))}
                    </div>
                  );
                })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// The query editor + results grid shown in the bottom dock.
export function QueryRunner({
  connection,
  initialSql,
}: {
  connection: string;
  initialSql: string;
}) {
  const [sql, setSql] = useState(initialSql);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setError(null);
    setBusy(true);
    try {
      setResult(await api.dbQuery(connection, sql));
    } catch (e) {
      setError(String(e));
      setResult(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-2 px-2 py-1.5 border-b border-border">
        <span className="text-fg-faint text-xs">{connection}</span>
        <button
          onClick={run}
          disabled={busy}
          className="text-xs px-2 py-0.5 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
        >
          ▷ Run
        </button>
        <span className="text-fg-faint text-xs">
          {result ? `${result.row_count} rows` : ""}
        </span>
      </div>
      <textarea
        value={sql}
        onChange={(e) => setSql(e.target.value)}
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === "Enter") run();
        }}
        spellCheck={false}
        className="h-20 bg-bg border-b border-border px-3 py-2 text-fg font-mono text-xs outline-none resize-none"
        placeholder="SELECT * FROM users LIMIT 100;   (Cmd/Ctrl+Enter to run)"
      />
      <div className="flex-1 overflow-auto">
        {error && <div className="px-3 py-2 text-danger text-xs">{error}</div>}
        {result && result.columns.length > 0 && (
          <table className="text-xs border-collapse w-full">
            <thead className="sticky top-0 bg-bg-elevated">
              <tr>
                {result.columns.map((c) => (
                  <th
                    key={c}
                    className="text-left px-2 py-1 border border-border text-fg-muted font-semibold whitespace-nowrap"
                  >
                    {c}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {result.rows.map((row, i) => (
                <tr key={i} className="hover:bg-bg-hover">
                  {row.map((cell, j) => (
                    <td
                      key={j}
                      className="px-2 py-1 border border-border/60 text-fg-muted whitespace-nowrap max-w-[280px] truncate"
                      title={cell}
                    >
                      {cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
