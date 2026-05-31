import { useMemo, useState } from "react";
import { api, type HttpResponse, type Route } from "../lib/api";

const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

const METHOD_COLOR: Record<string, string> = {
  GET: "#3fb950",
  POST: "#d29922",
  PUT: "#3574f0",
  PATCH: "#a371f7",
  DELETE: "#f85149",
  HEAD: "#8b949e",
  OPTIONS: "#8b949e",
};

type Header = { key: string; value: string };

function statusColor(s: number): string {
  if (s >= 500) return "#f85149";
  if (s >= 400) return "#d29922";
  if (s >= 300) return "#a371f7";
  if (s >= 200) return "#3fb950";
  return "#8b949e";
}

function prettify(body: string, contentType: string): string {
  if (/json/i.test(contentType)) {
    try {
      return JSON.stringify(JSON.parse(body), null, 2);
    } catch {
      /* leave as-is */
    }
  }
  return body;
}

export default function HttpClient({ routes }: { routes: Route[] }) {
  const [method, setMethod] = useState("GET");
  const [base, setBase] = useState("http://localhost:8000");
  const [path, setPath] = useState("/");
  const [headers, setHeaders] = useState<Header[]>([
    { key: "Accept", value: "application/json" },
  ]);
  const [body, setBody] = useState("");
  const [resp, setResp] = useState<HttpResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [tab, setTab] = useState<"body" | "headers">("body");
  const [showRoutes, setShowRoutes] = useState(false);

  // Route suggestions from the indexed Laravel route table.
  const suggestions = useMemo(() => {
    const q = path.replace(/^\//, "").toLowerCase();
    return routes
      .filter(
        (r) =>
          (method === "GET" || r.method.toUpperCase().includes(method)) &&
          (q === "" || r.uri.toLowerCase().includes(q))
      )
      .slice(0, 30);
  }, [routes, path, method]);

  const pickRoute = (r: Route) => {
    const m = r.method.split("|")[0].toUpperCase();
    if (METHODS.includes(m)) setMethod(m);
    setPath("/" + r.uri.replace(/^\//, ""));
    setShowRoutes(false);
  };

  const send = async () => {
    setLoading(true);
    setError(null);
    const url = base.replace(/\/$/, "") + "/" + path.replace(/^\//, "");
    try {
      const r = await api.httpRequest(
        method,
        url,
        headers.filter((h) => h.key.trim()).map((h) => [h.key, h.value] as [string, string]),
        method === "GET" || method === "HEAD" ? undefined : body
      );
      setResp(r);
      setTab("body");
    } catch (e) {
      setError(String(e));
      setResp(null);
    } finally {
      setLoading(false);
    }
  };

  const setHeader = (i: number, patch: Partial<Header>) =>
    setHeaders((hs) => hs.map((h, j) => (j === i ? { ...h, ...patch } : h)));

  const respContentType =
    resp?.headers.find(([k]) => k.toLowerCase() === "content-type")?.[1] ?? "";

  return (
    <div className="h-full flex flex-col text-xs">
      {/* request line */}
      <div className="flex items-center gap-1.5 px-2 py-1.5 border-b border-line">
        <select
          value={method}
          onChange={(e) => setMethod(e.target.value)}
          className="bg-bg-hover rounded px-1.5 py-1 font-semibold outline-none"
          style={{ color: METHOD_COLOR[method] }}
        >
          {METHODS.map((m) => (
            <option key={m} value={m} style={{ color: "#ddd" }}>
              {m}
            </option>
          ))}
        </select>
        <input
          value={base}
          onChange={(e) => setBase(e.target.value)}
          spellCheck={false}
          className="w-44 bg-bg-hover rounded px-2 py-1 text-fg-muted outline-none focus:ring-1 focus:ring-accent"
          placeholder="http://localhost:8000"
        />
        <div className="relative flex-1">
          <input
            value={path}
            onChange={(e) => {
              setPath(e.target.value);
              setShowRoutes(true);
            }}
            onFocus={() => setShowRoutes(true)}
            onBlur={() => setTimeout(() => setShowRoutes(false), 150)}
            spellCheck={false}
            className="w-full bg-bg-hover rounded px-2 py-1 font-mono outline-none focus:ring-1 focus:ring-accent"
            placeholder="/api/users"
          />
          {showRoutes && suggestions.length > 0 && (
            <div className="absolute z-50 top-full mt-1 left-0 right-0 max-h-60 overflow-auto rounded-lg border border-border bg-bg-panel shadow-2xl">
              {suggestions.map((r, i) => (
                <button
                  key={i}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    pickRoute(r);
                  }}
                  className="w-full flex items-center gap-2 px-2 py-1 text-left hover:bg-bg-hover"
                >
                  <span
                    className="font-semibold w-12 shrink-0"
                    style={{ color: METHOD_COLOR[r.method.split("|")[0].toUpperCase()] ?? "#8b949e" }}
                  >
                    {r.method.split("|")[0]}
                  </span>
                  <span className="font-mono text-fg-muted truncate">/{r.uri.replace(/^\//, "")}</span>
                  {r.name && <span className="ml-auto text-fg-faint truncate max-w-[30%]">{r.name}</span>}
                </button>
              ))}
            </div>
          )}
        </div>
        <button
          onClick={send}
          disabled={loading}
          className="px-3 py-1 rounded bg-accent text-white font-semibold hover:bg-accent/90 disabled:opacity-50"
        >
          {loading ? "…" : "Send"}
        </button>
      </div>

      {/* request body + headers split */}
      <div className="flex-1 min-h-0 grid grid-cols-2">
        <div className="flex flex-col border-r border-line min-h-0">
          <div className="flex items-center gap-3 px-2 py-1 border-b border-line text-fg-faint">
            <button
              onClick={() => setTab("body")}
              className={tab === "body" ? "text-fg" : "hover:text-fg"}
            >
              Body
            </button>
            <button
              onClick={() => setTab("headers")}
              className={tab === "headers" ? "text-fg" : "hover:text-fg"}
            >
              Headers ({headers.length})
            </button>
          </div>
          {tab === "body" ? (
            <textarea
              value={body}
              onChange={(e) => setBody(e.target.value)}
              spellCheck={false}
              placeholder='{ "key": "value" }'
              className="flex-1 min-h-0 resize-none bg-transparent p-2 font-mono text-fg-muted outline-none"
            />
          ) : (
            <div className="flex-1 min-h-0 overflow-auto p-1.5 space-y-1">
              {headers.map((h, i) => (
                <div key={i} className="flex items-center gap-1">
                  <input
                    value={h.key}
                    onChange={(e) => setHeader(i, { key: e.target.value })}
                    placeholder="Header"
                    className="w-1/3 bg-bg-hover rounded px-1.5 py-0.5 outline-none"
                  />
                  <input
                    value={h.value}
                    onChange={(e) => setHeader(i, { value: e.target.value })}
                    placeholder="Value"
                    className="flex-1 bg-bg-hover rounded px-1.5 py-0.5 font-mono outline-none"
                  />
                  <button
                    onClick={() => setHeaders((hs) => hs.filter((_, j) => j !== i))}
                    className="text-fg-faint hover:text-danger px-1"
                  >
                    ✕
                  </button>
                </div>
              ))}
              <button
                onClick={() => setHeaders((hs) => [...hs, { key: "", value: "" }])}
                className="text-accent hover:underline px-1 py-0.5"
              >
                + Add header
              </button>
            </div>
          )}
        </div>

        {/* response */}
        <div className="flex flex-col min-h-0">
          <div className="flex items-center gap-3 px-2 py-1 border-b border-line">
            {resp ? (
              <>
                <span className="font-semibold" style={{ color: statusColor(resp.status) }}>
                  {resp.status} {resp.status_text}
                </span>
                <span className="text-fg-faint">{resp.duration_ms} ms</span>
                <span className="text-fg-faint">{resp.size} B</span>
              </>
            ) : (
              <span className="text-fg-faint">Response</span>
            )}
          </div>
          <div className="flex-1 min-h-0 overflow-auto p-2 font-mono whitespace-pre-wrap break-words text-fg-muted">
            {error ? (
              <span className="text-danger">{error}</span>
            ) : resp ? (
              prettify(resp.body, respContentType)
            ) : (
              <span className="text-fg-faint">Send a request to see the response.</span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
