import type { Route } from "../lib/api";

const METHOD_COLOR: Record<string, string> = {
  GET: "#3fb950",
  POST: "#d29922",
  PUT: "#4c8bf5",
  PATCH: "#4c8bf5",
  DELETE: "#f85149",
  ANY: "#8b949e",
};

function methodColor(method: string): string {
  const first = method.split("|")[0];
  return METHOD_COLOR[first] || "#9d6cff";
}

// Laravel route list — navigates to the route definition on click.
export default function RoutesPanel({
  routes,
  onPick,
}: {
  routes: Route[];
  onPick: (file: string, line: number) => void;
}) {
  if (routes.length === 0) {
    return (
      <div className="px-3 py-4 text-fg-faint text-xs">
        No routes found. Open a Laravel project to see its routes.
      </div>
    );
  }
  return (
    <div className="overflow-y-auto h-full pb-4 text-sm">
      {routes.map((r, i) => (
        <div
          key={`${r.method}-${r.uri}-${i}`}
          className="row flex-col items-start gap-0.5 py-1.5"
          onClick={() => onPick(r.file, r.line)}
          title={r.action || ""}
        >
          <div className="flex items-center gap-2 w-full">
            <span
              className="text-[10px] font-bold w-12 shrink-0"
              style={{ color: methodColor(r.method) }}
            >
              {r.method}
            </span>
            <span className="truncate text-fg">{r.uri}</span>
          </div>
          {(r.name || r.action) && (
            <div className="pl-14 text-fg-faint text-xs truncate w-full">
              {r.name ? `${r.name} · ` : ""}
              {r.action || ""}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
