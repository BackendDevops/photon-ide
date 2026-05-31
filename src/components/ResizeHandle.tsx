import { useCallback } from "react";

// A thin draggable splitter. `dir="x"` resizes width (vertical handle),
// `dir="y"` resizes height (horizontal handle). Emits signed pixel deltas.
export default function ResizeHandle({
  dir,
  onDelta,
}: {
  dir: "x" | "y";
  onDelta: (delta: number) => void;
}) {
  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const start = dir === "x" ? e.clientX : e.clientY;
      let last = start;
      const move = (ev: MouseEvent) => {
        const cur = dir === "x" ? ev.clientX : ev.clientY;
        onDelta(cur - last);
        last = cur;
      };
      const up = () => {
        window.removeEventListener("mousemove", move);
        window.removeEventListener("mouseup", up);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };
      window.addEventListener("mousemove", move);
      window.addEventListener("mouseup", up);
      document.body.style.cursor = dir === "x" ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";
    },
    [dir, onDelta]
  );

  return (
    <div
      onMouseDown={onMouseDown}
      className={
        dir === "x"
          ? "w-1 cursor-col-resize hover:bg-accent/40 active:bg-accent/60 transition-colors shrink-0"
          : "h-1 cursor-row-resize hover:bg-accent/40 active:bg-accent/60 transition-colors shrink-0"
      }
    />
  );
}
