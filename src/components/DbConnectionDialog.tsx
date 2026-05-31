import { useState } from "react";
import { api, type DataSource } from "../lib/api";

const DRIVERS: DataSource["driver"][] = ["mysql", "mariadb", "postgres", "sqlite"];

function blank(): DataSource {
  return {
    id: "",
    name: "@localhost",
    driver: "mysql",
    host: "localhost",
    port: 3306,
    user: "",
    database: "",
    sqlite_path: "",
    save_password: true,
    password: "",
  };
}

// "Data Sources and Drivers"-style connection editor.
export default function DbConnectionDialog({
  initial,
  onClose,
  onSaved,
}: {
  initial: DataSource | null;
  onClose: () => void;
  onSaved: (sources: DataSource[]) => void;
}) {
  const [ds, setDs] = useState<DataSource>(initial ?? blank());
  const [test, setTest] = useState<string | null>(null);
  const [testOk, setTestOk] = useState(false);
  const [busy, setBusy] = useState(false);

  const set = <K extends keyof DataSource>(k: K, v: DataSource[K]) =>
    setDs((d) => ({ ...d, [k]: v }));

  const isSqlite = ds.driver === "sqlite";

  const url = isSqlite
    ? `sqlite://${ds.sqlite_path}?mode=rwc`
    : `${ds.driver === "postgres" ? "postgres" : "mysql"}://${ds.user}${
        ds.password ? ":•••" : ""
      }@${ds.host}:${ds.port}/${ds.database}`;

  const doTest = async () => {
    setBusy(true);
    setTest(null);
    try {
      const msg = await api.dbTestSource(ds, ds.password ?? undefined);
      setTest(msg);
      setTestOk(true);
    } catch (e) {
      setTest(String(e));
      setTestOk(false);
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    setBusy(true);
    try {
      const withId: DataSource = {
        ...ds,
        id: ds.id || `${ds.name}-${Date.now()}`,
      };
      const sources = await api.dbSaveSource(withId);
      onSaved(sources);
      onClose();
    } catch (e) {
      setTest(String(e));
      setTestOk(false);
    } finally {
      setBusy(false);
    }
  };

  const Field = ({
    label,
    children,
  }: {
    label: string;
    children: React.ReactNode;
  }) => (
    <label className="grid grid-cols-[110px_1fr] items-center gap-3 mb-2.5">
      <span className="text-fg-muted text-sm text-right">{label}</span>
      {children}
    </label>
  );

  const input =
    "bg-bg-elevated border border-border rounded px-2 py-1.5 text-sm text-fg outline-none focus:border-accent";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="pop-in w-[640px] max-w-[94vw] bg-bg-panel border border-border rounded-xl shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-border text-sm text-fg-muted">
          Data Sources and Drivers
        </div>
        <div className="p-4">
          <Field label="Name:">
            <input className={input} value={ds.name} onChange={(e) => set("name", e.target.value)} />
          </Field>
          <Field label="Driver:">
            <select
              className={input}
              value={ds.driver}
              onChange={(e) => {
                const driver = e.target.value as DataSource["driver"];
                set("driver", driver);
                if (driver === "postgres" && ds.port === 3306) set("port", 5432);
                if ((driver === "mysql" || driver === "mariadb") && ds.port === 5432)
                  set("port", 3306);
              }}
            >
              {DRIVERS.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </Field>

          {isSqlite ? (
            <Field label="File path:">
              <input
                className={input}
                placeholder="/absolute/path/to/database.sqlite"
                value={ds.sqlite_path}
                onChange={(e) => set("sqlite_path", e.target.value)}
              />
            </Field>
          ) : (
            <>
              <div className="grid grid-cols-[110px_1fr_70px_90px] items-center gap-3 mb-2.5">
                <span className="text-fg-muted text-sm text-right">Host:</span>
                <input className={input} value={ds.host} onChange={(e) => set("host", e.target.value)} />
                <span className="text-fg-muted text-sm text-right">Port:</span>
                <input
                  className={input}
                  type="number"
                  value={ds.port}
                  onChange={(e) => set("port", Number(e.target.value))}
                />
              </div>
              <Field label="User:">
                <input className={input} value={ds.user} onChange={(e) => set("user", e.target.value)} />
              </Field>
              <Field label="Password:">
                <div className="flex items-center gap-3">
                  <input
                    className={`${input} flex-1`}
                    type="password"
                    value={ds.password ?? ""}
                    onChange={(e) => set("password", e.target.value)}
                  />
                  <label className="flex items-center gap-1.5 text-xs text-fg-muted whitespace-nowrap">
                    <input
                      type="checkbox"
                      checked={ds.save_password}
                      onChange={(e) => set("save_password", e.target.checked)}
                    />
                    Save
                  </label>
                </div>
              </Field>
              <Field label="Database:">
                <input className={input} value={ds.database} onChange={(e) => set("database", e.target.value)} />
              </Field>
            </>
          )}

          <Field label="URL:">
            <code className="text-xs text-fg-faint truncate">{url}</code>
          </Field>

          {ds.save_password && !isSqlite && (
            <div className="text-warn text-[11px] mb-2">
              ⚠ Saved passwords are stored in <code>.photon/datasources.json</code>{" "}
              (plaintext in v1; OS-keychain is planned — docs/09).
            </div>
          )}

          {test && (
            <div className={`text-xs mb-2 ${testOk ? "text-success" : "text-danger"}`}>
              {testOk ? "✓ " : "✕ "}
              {test}
            </div>
          )}
        </div>

        <div className="flex items-center px-4 py-3 border-t border-border">
          <button
            onClick={doTest}
            disabled={busy}
            className="text-sm text-accent hover:underline disabled:opacity-40"
          >
            Test Connection
          </button>
          <div className="flex-1" />
          <button onClick={onClose} className="text-sm px-3 py-1.5 rounded text-fg-muted hover:bg-bg-hover">
            Cancel
          </button>
          <button
            onClick={save}
            disabled={busy || !ds.name}
            className="text-sm px-3 py-1.5 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40 ml-2"
          >
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
