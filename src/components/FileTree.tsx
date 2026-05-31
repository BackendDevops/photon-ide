import { useMemo, useState } from "react";
import type { FileEntry } from "../lib/api";
import { FileIcon, FolderIcon } from "./icons";

interface TreeNode {
  name: string;
  path: string;
  dir: boolean;
  lang?: string;
  children: Map<string, TreeNode>;
}

function buildTree(files: FileEntry[]): TreeNode {
  const root: TreeNode = {
    name: "",
    path: "",
    dir: true,
    children: new Map(),
  };
  for (const f of files) {
    const parts = f.path.split("/");
    let node = root;
    parts.forEach((part, i) => {
      const isLeaf = i === parts.length - 1;
      let child = node.children.get(part);
      if (!child) {
        child = {
          name: part,
          path: parts.slice(0, i + 1).join("/"),
          dir: !isLeaf,
          lang: isLeaf ? f.lang : undefined,
          children: new Map(),
        };
        node.children.set(part, child);
      }
      node = child;
    });
  }
  return root;
}

const LIBRARY_DIRS = new Set(["vendor", "node_modules"]);

function sortedChildren(node: TreeNode): TreeNode[] {
  return [...node.children.values()].sort((a, b) => {
    // Library dirs sink to the bottom of their level.
    const al = a.dir && LIBRARY_DIRS.has(a.name);
    const bl = b.dir && LIBRARY_DIRS.has(b.name);
    if (al !== bl) return al ? 1 : -1;
    if (a.dir !== b.dir) return a.dir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

function Row({
  node,
  depth,
  activePath,
  onOpen,
  lib,
}: {
  node: TreeNode;
  depth: number;
  activePath: string | null;
  onOpen: (path: string) => void;
  lib?: boolean;
}) {
  // All folders start collapsed; the user expands what they need.
  const [open, setOpen] = useState(false);
  const pad = { paddingLeft: `${8 + depth * 12}px` };
  // Vendor / node_modules and everything inside them is "library code".
  const inLib = lib || (node.dir && LIBRARY_DIRS.has(node.name));

  if (node.dir) {
    return (
      <>
        <div
          className="row text-fg-muted"
          style={{ ...pad, opacity: inLib ? 0.55 : 1 }}
          onClick={() => setOpen((o) => !o)}
        >
          <span className="text-fg-faint text-[10px] w-3">
            {open ? "▾" : "▸"}
          </span>
          <FolderIcon open={open} root={depth === 0} />
          <span className="truncate">{node.name}</span>
          {node.dir && LIBRARY_DIRS.has(node.name) && (
            <span className="ml-auto mr-1 text-2xs text-fg-faint uppercase tracking-wider">library</span>
          )}
        </div>
        {open &&
          sortedChildren(node).map((c) => (
            <Row
              key={c.path}
              node={c}
              depth={depth + 1}
              activePath={activePath}
              onOpen={onOpen}
              lib={inLib}
            />
          ))}
      </>
    );
  }

  return (
    <div
      className={`row ${activePath === node.path ? "row-active" : ""}`}
      style={{ ...pad, opacity: inLib ? 0.55 : 1 }}
      onClick={() => onOpen(node.path)}
      title={node.path}
    >
      <span className="w-3" />
      <FileIcon lang={node.lang} name={node.name} />
      <span className="truncate">{node.name}</span>
    </div>
  );
}

export default function FileTree({
  files,
  activePath,
  onOpen,
}: {
  files: FileEntry[];
  activePath: string | null;
  onOpen: (path: string) => void;
}) {
  const tree = useMemo(() => buildTree(files), [files]);
  const top = sortedChildren(tree);
  return (
    <div className="overflow-y-auto h-full pb-4 text-sm">
      {top.map((c) => (
        <Row
          key={c.path}
          node={c}
          depth={0}
          activePath={activePath}
          onOpen={onOpen}
        />
      ))}
    </div>
  );
}
