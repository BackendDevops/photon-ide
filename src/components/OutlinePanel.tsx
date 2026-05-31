import type { Symbol } from "../lib/api";
import { KindBadge } from "./icons";

// Structure view for the active file — the symbols extracted by the PHP engine.
export default function OutlinePanel({
  symbols,
  onPick,
}: {
  symbols: Symbol[];
  onPick: (line: number) => void;
}) {
  if (symbols.length === 0) {
    return (
      <div className="px-3 py-4 text-fg-faint text-xs">
        No symbols. Open a PHP file to see its structure.
      </div>
    );
  }
  return (
    <div className="overflow-y-auto h-full pb-4 text-sm">
      {symbols.map((s, i) => {
        // Indent members (those with a container) one level.
        const indent = s.container && s.kind !== "class" ? 20 : 8;
        return (
          <div
            key={`${s.name}-${s.line}-${i}`}
            className="row"
            style={{ paddingLeft: `${indent}px` }}
            onClick={() => onPick(s.line)}
            title={s.fqn || s.name}
          >
            <KindBadge kind={s.kind} />
            <span className="truncate">{s.name}</span>
            {s.container && s.kind !== "class" && (
              <span className="text-fg-faint text-xs truncate">
                {s.container}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
