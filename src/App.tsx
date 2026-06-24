import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  monacoLang,
  type DataSource,
  type FileEntry,
  type ProjectSummary,
  type Reference,
  type Route,
  type Symbol,
} from "./lib/api";
import { useSettings, saveSettings } from "./lib/settings";
import Header from "./components/Header";
import FileTree from "./components/FileTree";
import EditorPane from "./components/EditorPane";
import SearchEverywhere, { type PaletteAction } from "./components/SearchEverywhere";
import RecentPopup, { type RecentLocation } from "./components/RecentPopup";
import OutlinePanel from "./components/OutlinePanel";
import RoutesPanel from "./components/RoutesPanel";
import StatusBar from "./components/StatusBar";
import InlineRename from "./components/InlineRename";
import UsagesPanel from "./components/UsagesPanel";
import ModelsPanel from "./components/ModelsPanel";
import DbPanel, { QueryRunner } from "./components/DbPanel";
import DbConnectionDialog from "./components/DbConnectionDialog";
import GitSidebar from "./components/GitSidebar";
import CommitGraph from "./components/CommitGraph";
import ConflictCenter from "./components/ConflictCenter";
import InsightsPanel from "./components/InsightsPanel";
import RebaseModal from "./components/RebaseModal";
import HistoryPopup from "./components/HistoryPopup";
import DebugPanel from "./components/DebugPanel";
import TerminalDock from "./components/Terminal";
import HttpClient from "./components/HttpClient";
import ArchMap from "./components/ArchMap";
import SettingsDialog from "./components/SettingsDialog";
import CommunityHub from "./components/CommunityHub";
import TemplateDialog from "./components/TemplateDialog";
import ExtensionsPanel from "./components/ExtensionsPanel";
import AiPanel from "./components/AiPanel";
import ArtisanRunner from "./components/ArtisanRunner";
import DiffViewer from "./components/DiffViewer";
import UsagesPopup from "./components/UsagesPopup";
import ResizeHandle from "./components/ResizeHandle";
import type { UsagesResult, RootInfo } from "./lib/api";

type SidebarView = "files" | "outline" | "routes" | "models" | "db" | "git" | "debug" | "extensions" | "ai";
type DockTab = "terminal" | "http" | "debug";
type BottomPanel =
  | { kind: "usages"; name: string; list: Reference[] }
  | { kind: "query"; connection: string; sql: string }
  | { kind: "diff"; file: string; original: string; modified: string }
  | null;

export default function App() {
  const settings = useSettings();
  const [summary, setSummary] = useState<ProjectSummary | null>(null);
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [routes, setRoutes] = useState<Route[]>([]);
  const [branch, setBranch] = useState<string | null>(null);
  const [indexing, setIndexing] = useState(false);

  const [tabs, setTabs] = useState<{ path: string }[]>([]);
  const [active, setActive] = useState<string | null>(null);
  // Buffers live in a ref so keystroke handlers never trigger React re-renders.
  // `setBuffers` (state) has been removed; render-time reads use buffersRef.current.
  const buffersRef = useRef<Record<string, string>>({});
  const syncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const autoSaveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [dirty, setDirty] = useState<Record<string, boolean>>({});
  const [symbols, setSymbols] = useState<Symbol[]>([]);

  const [sidebar, setSidebar] = useState<SidebarView>("files");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [recentOpen, setRecentOpen] = useState(false);
  const [recentFiles, setRecentFiles] = useState<string[]>([]);
  const [recentLocations, setRecentLocations] = useState<RecentLocation[]>([]);
  const [reveal, setReveal] = useState<{ line: number; nonce: number } | null>(null);

  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [bottom, setBottom] = useState<BottomPanel>(null);
  const [gitRefresh, setGitRefresh] = useState(0);
  const [conflictFile, setConflictFile] = useState<string | null>(null);
  const [gitView, setGitView] = useState<"graph" | "insights">("graph");
  const [rebaseBase, setRebaseBase] = useState<string | null>(null);
  const [historyPath, setHistoryPath] = useState<string | null>(null);
  const [crumb, setCrumb] = useState<string | null>(null);

  // Immediate children of a breadcrumb prefix (for the dropdown).
  const crumbChildren = useMemo(() => {
    if (!crumb) return [] as { name: string; dir: boolean; full: string }[];
    const seen = new Set<string>();
    const out: { name: string; dir: boolean; full: string }[] = [];
    for (const f of files) {
      if (!f.path.startsWith(crumb + "/")) continue;
      const rest = f.path.slice(crumb.length + 1);
      const seg = rest.split("/")[0];
      if (seen.has(seg)) continue;
      seen.add(seg);
      out.push({ name: seg, dir: rest.includes("/"), full: `${crumb}/${seg}` });
    }
    out.sort((a, b) => (a.dir !== b.dir ? (a.dir ? -1 : 1) : a.name.localeCompare(b.name)));
    return out;
  }, [crumb, files]);
  // Devcontainer / Docker detection → top-bar "Run in Container" pill.
  const container = useMemo(() => {
    const has = (re: RegExp) => files.some((f) => re.test(f.path));
    if (has(/(^|\/)\.devcontainer\/devcontainer\.json$/) || has(/(^|\/)devcontainer\.json$/))
      return "devcontainer";
    if (has(/(^|\/)(docker-)?compose\.ya?ml$/)) return "compose";
    if (has(/(^|\/)Dockerfile$/)) return "docker";
    return null as null | "devcontainer" | "compose" | "docker";
  }, [files]);

  const [breakpoints, setBreakpoints] = useState<{ path: string; line: number; condition?: string }[]>([]);
  const [aiPending, setAiPending] = useState<{ text: string; nonce: number } | null>(null);
  const [debug, setDebug] = useState<{ active: boolean; file: string | null; line: number | null; inline: string | null }>({ active: false, file: null, line: null, inline: null });

  useEffect(() => {
    const subs = [
      listen<string>("xdebug-status", (e) => {
        const on = !["stopped", "idle", "finished"].includes(e.payload);
        setDebug((d) => ({ ...d, active: on }));
      }),
      listen("xdebug-end", () => setDebug({ active: false, file: null, line: null, inline: null })),
      listen<import("./lib/api").DebugBreak>("xdebug-break", async (e) => {
        let wp: string | null = null;
        try { wp = await api.pathToWorkspace(e.payload.file); } catch { /* */ }
        const inline = e.payload.vars
          .filter((v) => ["int", "string", "bool", "float"].includes(v.ty))
          .slice(0, 4)
          .map((v) => `${v.name} = ${v.value}`)
          .join("   ");
        setDebug({ active: true, file: wp, line: e.payload.line, inline });
      }),
    ];
    return () => { subs.forEach((s) => s.then((u) => u())); };
  }, []);

  const askAi = useCallback((text: string) => {
    setAiPending({ text, nonce: Date.now() });
    setSidebar("ai");
  }, []);

  const toggleBreakpoint = useCallback((path: string, line: number) => {
    setBreakpoints((bps) => {
      const exists = bps.some((b) => b.path === path && b.line === line);
      if (exists) {
        void api.debugRemoveBreakpoint(path, line).catch(() => {});
        return bps.filter((b) => !(b.path === path && b.line === line));
      }
      void api.debugSetBreakpoint(path, line).catch(() => {});
      return [...bps, { path, line }];
    });
  }, []);

  const conditionalBreakpoint = useCallback((path: string, line: number) => {
    setPrompt({
      label: "Breakpoint condition (PHP expression)",
      value: "",
      onOk: (expr) => {
        void api.debugSetBreakpoint(path, line, expr).catch(() => {});
        setBreakpoints((bps) => [
          ...bps.filter((b) => !(b.path === path && b.line === line)),
          { path, line, condition: expr },
        ]);
      },
    });
  }, []);
  const [dock, setDock] = useState<DockTab | null>(null);
  const toggleTerminal = useCallback(
    () => setDock((d) => (d === "terminal" ? null : "terminal")),
    []
  );
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [communityOpen, setCommunityOpen] = useState(false);
  const [templateOpen, setTemplateOpen] = useState(false);
  const [artisanOpen, setArtisanOpen] = useState(false);
  const [dbDialog, setDbDialog] = useState<{ open: boolean; initial: DataSource | null }>({
    open: false,
    initial: null,
  });
  const [dbRefresh, setDbRefresh] = useState(0);
  const [lintKey, setLintKey] = useState(0);
  const [usagesPopup, setUsagesPopup] = useState<{ result: UsagesResult; x: number; y: number } | null>(null);
  const [projects, setProjects] = useState<RootInfo[]>([]);
  const [sidebarWidth, setSidebarWidth] = useState(264);
  const [bottomHeight, setBottomHeight] = useState(240);
  const [terminalHeight, setTerminalHeight] = useState(260);
  const [rightTool, setRightTool] = useState<"arch" | "outline" | null>(null);
  const [rightWidth, setRightWidth] = useState(320);
  // Zero-Mouse Flow — which UI region currently holds keyboard focus (F6 cycles).
  const [focusRegion, setFocusRegion] = useState<"sidebar" | "editor" | "dock" | "right">("editor");
  const [prompt, setPrompt] = useState<{ label: string; value: string; onOk: (v: string) => void } | null>(null);
  const [toasts, setToasts] = useState<{ id: number; msg: string; kind: "success" | "error" | "info" }[]>([]);

  const showToast = useCallback((msg: string, kind?: "success" | "error" | "info") => {
    const k =
      kind ??
      (/(fail|error|denied|unable|could not|conflict|✕)/i.test(msg)
        ? "error"
        : /(✓|passed|connected|pushed|pulled|committed|amended|switched|created|renamed|moved|inlined|deleted|generated|stashed|extracted|updated|restored|indexed|saved)/i.test(msg)
        ? "success"
        : "info");
    const id = Date.now() + Math.random();
    setToasts((list) => [...list, { id, msg, kind: k }].slice(-4));
    setTimeout(() => setToasts((list) => list.filter((t) => t.id !== id)), 3400);
  }, []);

  // Test ▶ glyphs: methods/classes in a test file.
  const testLines = useMemo(() => {
    if (!active) return [] as number[];
    const isTest =
      /(^|\/)tests?\//i.test(active) ||
      symbols.some((s) => s.kind === "class" && s.name.endsWith("Test"));
    if (!isTest) return [];
    return symbols.filter((s) => s.kind === "method" || s.kind === "class").map((s) => s.line);
  }, [active, symbols]);

  const runTestAt = useCallback(
    (line: number) => {
      if (!active) return;
      const sym = symbols.find((s) => s.line === line);
      const filter = sym && sym.kind === "method" ? sym.name : undefined;
      const label = filter ?? active.split("/").pop();
      showToast(`Running ${label}…`);
      api
        .runTest(active, filter)
        .then((r) => showToast(`${r.passed ? "✓ Passed" : "✕ Failed"} · ${label}`))
        .catch((e) => showToast(String(e)));
    },
    [active, symbols, showToast]
  );

  // Clear any pending autosave/sync timers when the active tab/file changes.
  useEffect(() => {
    return () => {
      if (autoSaveTimeoutRef.current) {
        clearTimeout(autoSaveTimeoutRef.current);
      }
      if (syncTimeoutRef.current) {
        clearTimeout(syncTimeoutRef.current);
      }
    };
  }, [active]);

  // NOTE: CSS `zoom` breaks Monaco's mouse hit-testing (clicks land at the
  // wrong caret position). We clear any previously-applied zoom and scale the
  // UI through font sizes instead, so editor mouse interaction stays correct.
  useEffect(() => {
    (document.body.style as CSSStyleDeclaration & { zoom?: string }).zoom = "";
    const scale = Math.max(0.85, Math.min(1.5, settings.uiScale || 1));
    document.documentElement.style.setProperty("--ui-scale", String(scale));
    document.body.style.fontSize = `${15 * scale}px`;
  }, [settings.uiScale]);

  const langOf = (path: string) => files.find((f) => f.path === path)?.lang ?? "other";

  const refreshBranch = useCallback(async () => {
    try {
      const st = await api.gitStatus();
      setBranch(st.branch);
    } catch {
      setBranch(null);
    }
  }, []);

  const chooseFolder = useCallback(async () => {
    const picked = await openDialog({ directory: true, multiple: false });
    if (!picked || typeof picked !== "string") return;
    setIndexing(true);
    try {
      const s = await api.openProject(picked); // ADDS a root (multi-project)
      setSummary(s);
      const [f, r, p] = await Promise.all([
        api.listFiles(),
        api.listRoutes(),
        api.listProjects(),
      ]);
      setFiles(f);
      setRoutes(r);
      setProjects(p);
      void refreshBranch();
      // Deferred, declaration-level framework indexing (does not block open).
      void api
        .indexVendor()
        .then((n) => n && showToast(`Framework indexed: ${n} files`))
        .catch(() => {});
    } catch (e) {
      console.error(e);
    } finally {
      setIndexing(false);
    }
  }, [refreshBranch, showToast]);

  const closeProject = useCallback(async (label: string) => {
    try {
      const s = await api.closeProject(label);
      setSummary(s);
      const [f, r, p] = await Promise.all([
        api.listFiles(),
        api.listRoutes(),
        api.listProjects(),
      ]);
      setFiles(f);
      setRoutes(r);
      setProjects(p);
      // close tabs that belonged to the removed root
      setTabs((t) => t.filter((x) => !x.path.startsWith(`${label}/`)));
      setActive((a) => (a && a.startsWith(`${label}/`) ? null : a));
    } catch (e) {
      console.error(e);
    }
  }, []);

  const refreshSymbols = useCallback(async (path: string) => {
    try {
      setSymbols(await api.fileSymbols(path));
    } catch {
      setSymbols([]);
    }
  }, []);

  const openFile = useCallback(
    async (path: string, line?: number) => {
      // Cancel any pending debounced sync when switching files.
      if (syncTimeoutRef.current) {
        clearTimeout(syncTimeoutRef.current);
        syncTimeoutRef.current = null;
      }

      if (buffersRef.current[path] === undefined) {
        try {
          const content = await api.readFile(path);
          buffersRef.current[path] = content;
        } catch (e) {
          console.error(e);
          return;
        }
      }
      setTabs((t) => (t.some((x) => x.path === path) ? t : [...t, { path }]));
      setActive(path);
      if (sidebar === "git") setSidebar("files");
      void refreshSymbols(path);
      if (line) setReveal({ line, nonce: Date.now() });
      // Recent files (MRU) + recent locations.
      setRecentFiles((r) => [path, ...r.filter((p) => p !== path)].slice(0, 30));
      if (line && line > 1) {
        setRecentLocations((r) =>
          [{ file: path, line }, ...r.filter((x) => !(x.file === path && x.line === line))].slice(0, 30)
        );
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [active, refreshSymbols, sidebar]
  );

  const closeTab = (path: string) => {
    setTabs((t) => {
      const next = t.filter((x) => x.path !== path);
      if (active === path) setActive(next.length ? next[next.length - 1].path : null);
      return next;
    });
  };

  // ---- Branch-aware workspace memory ----
  // Open tabs + active file are remembered per (project, branch) so switching
  // branches restores the exact set of files you were working on there.
  const prevBranchRef = useRef<string | null>(null);
  const tabsKey = (root: string, br: string) => `photon:tabs:${root}:${br}`;

  // Persist the current tab set under the active branch.
  useEffect(() => {
    if (!summary?.root || !branch) return;
    try {
      localStorage.setItem(
        tabsKey(summary.root, branch),
        JSON.stringify({ tabs: tabs.map((t) => t.path), active })
      );
    } catch {
      /* storage unavailable */
    }
  }, [tabs, active, branch, summary?.root]);

  // Restore the remembered tab set when the branch changes.
  useEffect(() => {
    if (!summary?.root || !branch) return;
    const switching = prevBranchRef.current !== null && prevBranchRef.current !== branch;
    prevBranchRef.current = branch;
    if (!switching) return;
    try {
      const raw = localStorage.getItem(tabsKey(summary.root, branch));
      if (!raw) return;
      const saved = JSON.parse(raw) as { tabs: string[]; active: string | null };
      setTabs(saved.tabs.map((p) => ({ path: p })));
      if (saved.active) void openFile(saved.active);
      else setActive(null);
    } catch {
      /* ignore malformed memory */
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [branch, summary?.root]);

  const save = useCallback(async () => {
    if (!active) return;
    const content = buffersRef.current[active] ?? "";
    try {
      await api.saveFile(active, content);
      setDirty((d) => ({ ...d, [active]: false }));
      void refreshSymbols(active);
      setRoutes(await api.listRoutes());
      setLintKey((n) => n + 1); // re-run diagnostics against fresh index
    } catch (e) {
      console.error(e);
    }
  }, [active, refreshSymbols]);

  const onEditorChange = useCallback((v: string) => {
    if (!active) return;
    buffersRef.current[active] = v;
    // Use functional update so we don't need `dirty` in the dependency list,
    // preventing the closure from re-creating on every state change.
    setDirty((d) => d[active] ? d : { ...d, [active]: true });

    // 400ms debounce: only triggers re-render for outline/AI panels that need
    // buffer content — editor itself reads from buffersRef.current directly.
    if (syncTimeoutRef.current) clearTimeout(syncTimeoutRef.current);
    syncTimeoutRef.current = setTimeout(() => {
      // Force a render so dependents (e.g. AI panel) get the latest content.
      setDirty((d) => ({ ...d }));
    }, 400);

    if (settings.autoSave) {
      if (autoSaveTimeoutRef.current) clearTimeout(autoSaveTimeoutRef.current);
      autoSaveTimeoutRef.current = setTimeout(() => {
        void save();
      }, settings.autoSaveDelayMs);
    }
  }, [active, settings.autoSave, settings.autoSaveDelayMs, save]);


  const gotoFileLine = useCallback((file: string, line: number) => void openFile(file, line), [openFile]);

  // Context bundle for the AI panel: active file + project facts.
  const getAiContext = useCallback(() => {
    const parts: string[] = [];
    if (summary) {
      parts.push(
        `Project: ${summary.is_laravel ? "Laravel" : "PHP"} · ${summary.symbols} symbols · ${summary.routes} routes · ${summary.models} models.`
      );
    }
    if (active) {
      const body = (buffersRef.current[active] ?? "").slice(0, 6000);
      const lang = langOf(active);
      parts.push(`Active file: ${active}\n\`\`\`${lang}\n${body}\n\`\`\``);
    }
    return parts.join("\n\n");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [summary, active]);

  // F12 → Go to Definition (resolves into vendor/framework symbols too).
  const gotoDefinition = useCallback(
    async (word: string) => {
      try {
        const defs = await api.gotoSymbol(word);
        if (defs[0]) void openFile(defs[0].file, defs[0].line);
        else showToast(`No definition for ${word}`);
      } catch (e) {
        showToast(String(e));
      }
    },
    [openFile, showToast]
  );

  // Cmd+Alt+B → Go to Implementation(s).
  const gotoImplementation = useCallback(
    async (word: string) => {
      try {
        const res = await api.gotoImplementations(word);
        if (res.hits.length === 0) showToast(`No implementations of ${word}`);
        else if (res.hits.length === 1) void openFile(res.hits[0].file, res.hits[0].line);
        else setUsagesPopup({ result: res, x: window.innerWidth / 2 - 320, y: 140 });
      } catch (e) {
        showToast(String(e));
      }
    },
    [openFile, showToast]
  );

  const gotoType = useCallback(
    async (chain: string, offset: number) => {
      if (!active) return;
      try {
        const loc = await api.gotoType(active, offset, chain);
        if (loc) void openFile(loc.file, loc.line);
        else showToast("Type not resolved");
      } catch (e) {
        showToast(String(e));
      }
    },
    [active, openFile, showToast]
  );

  // Cmd+click on app(Foo::class) → concrete binding (or class definition).
  const resolveBinding = useCallback(
    async (word: string) => {
      try {
        const loc = await api.gotoBinding(word);
        if (loc) {
          void openFile(loc.file, loc.line);
          return;
        }
        const defs = await api.gotoSymbol(word);
        if (defs[0]) void openFile(defs[0].file, defs[0].line);
        else showToast(`No binding/definition for ${word}`);
      } catch (e) {
        showToast(String(e));
      }
    },
    [openFile, showToast]
  );

  // Cmd+click on a config()/route()/__() key → jump to its definition.
  const resolveKey = useCallback(
    async (kind: string, key: string) => {
      try {
        const loc = await api.gotoLaravelKey(kind, key);
        if (loc) void openFile(loc.file, loc.line);
        else showToast(`No definition for ${kind} key '${key}'`);
      } catch (e) {
        showToast(String(e));
      }
    },
    [openFile, showToast]
  );

  // Cmd/Ctrl+click — JetBrains-style: on a declaration → Find Usages popup;
  // on a use-site → go to the (symbol-resolved) definition.
  const cmdClick = useCallback(
    async (info: { word: string; line: number; offset: number; x: number; y: number; chain: string | null }) => {
      const { word, line, offset, x, y, chain } = info;
      try {
        // Is the click on a declaration in the active file? (symbol on this line)
        const onDeclaration = symbols.some((s) => s.name === word && s.line === line);
        if (onDeclaration) {
          setUsagesPopup({ result: await api.usagesPopup(word), x, y });
          return;
        }
        // Use-site → go to definition. Member access uses receiver-aware lookup.
        if (chain && active) {
          const loc = await api.gotoMemberDef(active, offset, chain, word);
          if (loc) {
            void openFile(loc.file, loc.line);
            return;
          }
        }
        const defs = await api.gotoSymbol(word);
        if (defs[0]) void openFile(defs[0].file, defs[0].line);
        else setUsagesPopup({ result: await api.usagesPopup(word), x, y });
      } catch {
        /* ignore */
      }
    },
    [symbols, active, openFile]
  );

  const findUsages = useCallback(async (word: string) => {
    try {
      setBottom({ kind: "usages", name: word, list: await api.findUsages(word) });
    } catch (e) {
      console.error(e);
    }
  }, []);

  const onRenameApplied = useCallback(async () => {
    const fresh: Record<string, string> = {};
    for (const p of Object.keys(buffersRef.current)) {
      try {
        const content = await api.readFile(p);
        fresh[p] = content;
        buffersRef.current[p] = content;
      } catch {
        /* moved */
      }
    }
    if (active) void refreshSymbols(active);
  }, [active, refreshSymbols]);

  const openDiff = useCallback(async (file: string) => {
    try {
      const sides = await api.gitDiffSides(file);
      setBottom({ kind: "diff", file, original: sides.original, modified: sides.modified });
    } catch (e) {
      console.error(e);
    }
  }, []);

  // ---- editor refactorings (extract / inline / safe-delete) ----
  const applyAndReload = useCallback(async (cs: import("./lib/api").ChangeSet) => {
    await api.applyChangeset(cs);
    await onRenameApplied();
    setLintKey((n) => n + 1);
  }, [onRenameApplied]);

  const extractVariable = useCallback(
    (sel: { start: number; end: number; line: number }) => {
      if (!active) return;
      setPrompt({
        label: "Variable name",
        value: "result",
        onOk: async (name) => {
          try {
            await save();
            const cs = await api.refactorExtractVariable(active, sel.start, sel.end, name, sel.line);
            await applyAndReload(cs);
            showToast("Extracted variable");
          } catch (e) {
            showToast(String(e));
          }
        },
      });
    },
    [active, save, applyAndReload, showToast]
  );

  const extractMethod = useCallback(
    (sel: { start: number; end: number; line: number }) => {
      if (!active) return;
      setPrompt({
        label: "Method name",
        value: "extracted",
        onOk: async (name) => {
          try {
            await save();
            const cs = await api.refactorExtractMethod(active, sel.start, sel.end, name, sel.line);
            await applyAndReload(cs);
            showToast("Extracted method");
          } catch (e) {
            showToast(String(e));
          }
        },
      });
    },
    [active, save, applyAndReload, showToast]
  );

  const changeSignature = useCallback(
    (line: number, params: string) => {
      if (!active) return;
      setPrompt({
        label: "Parameters",
        value: params,
        onOk: async (np) => {
          try {
            await save();
            const cs = await api.planChangeSignature(active, line, np);
            await applyAndReload(cs);
            showToast("Signature changed");
          } catch (e) {
            showToast(String(e));
          }
        },
      });
    },
    [active, save, applyAndReload, showToast]
  );

  const moveClass = useCallback(
    async (word: string) => {
      let oldNs = "";
      try {
        const syms = await api.gotoSymbol(word);
        const fqn = syms.find((s) => s.fqn)?.fqn;
        if (fqn) oldNs = fqn.split("\\").slice(0, -1).join("\\");
      } catch {
        /* ignore */
      }
      setPrompt({
        label: `New namespace for ${word}`,
        value: oldNs,
        onOk: async (ns) => {
          try {
            await save();
            const cs = await api.planMoveClass(word, ns);
            await applyAndReload(cs);
            showToast(`Moved ${word} → ${ns || "global"}`);
          } catch (e) {
            showToast(String(e));
          }
        },
      });
    },
    [save, applyAndReload, showToast]
  );

  const inlineVariable = useCallback(
    async (word: string) => {
      if (!active) return;
      try {
        await save();
        const cs = await api.refactorInlineVariable(active, word);
        await applyAndReload(cs);
        showToast(`Inlined ${word}`);
      } catch (e) {
        showToast(String(e));
      }
    },
    [active, save, applyAndReload, showToast]
  );

  const safeDelete = useCallback(
    async (word: string) => {
      try {
        const cs = await api.refactorSafeDelete(word);
        await applyAndReload(cs);
        showToast(`Deleted ${word}`);
      } catch (e) {
        showToast(String(e));
      }
    },
    [applyAndReload, showToast]
  );

  const newBranch = useCallback(() => {
    setPrompt({
      label: "New branch name",
      value: "",
      onOk: async (name) => {
        if (!name.trim()) return;
        try {
          await api.gitCreateBranch(name.trim());
          await refreshBranch();
          setGitRefresh((n) => n + 1);
          showToast(`Created branch ${name}`);
        } catch (e) {
          showToast(String(e));
        }
      },
    });
  }, [refreshBranch, showToast]);

  const checkout = useCallback(
    async (b: string) => {
      try {
        await api.gitCheckout(b);
        await refreshBranch();
        setGitRefresh((n) => n + 1);
        showToast(`Switched to ${b}`);
      } catch (e) {
        showToast(String(e));
      }
    },
    [refreshBranch, showToast]
  );

  const gitAction = useCallback(
    (a: "update" | "commit" | "push") => {
      if (a === "commit") setSidebar("git");
      else if (a === "push") api.gitPush().then(() => showToast("Pushed")).catch((e) => showToast(String(e)));
      else api.gitUpdate().then(() => { showToast("Project updated"); setGitRefresh((n) => n + 1); }).catch((e) => showToast(String(e)));
    },
    [showToast]
  );

  // double-shift = Search Everywhere; Cmd/Ctrl+P / +S / +`
  const lastShift = useRef(0);
  useEffect(() => {
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === "Shift") {
        const now = Date.now();
        if (now - lastShift.current < 350) {
          setPaletteOpen(true);
          lastShift.current = 0;
        } else lastShift.current = now;
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "p") {
        e.preventDefault();
        setPaletteOpen(true);
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "`") {
        e.preventDefault();
        toggleTerminal();
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "e") {
        e.preventDefault();
        setRecentOpen(true);
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        if (summary) setTemplateOpen(true);
      }
    };
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [save, summary]);

  // Zero-Mouse Flow — F6 cycles keyboard focus across visible regions
  // (Shift+F6 reverses). Focus moves into the region's DOM so Tab works within.
  useEffect(() => {
    const regions: ("sidebar" | "editor" | "dock" | "right")[] = [
      "sidebar",
      "editor",
      ...(dock ? (["dock"] as const) : []),
      ...(rightTool ? (["right"] as const) : []),
    ];
    const focusEl = (r: string) => {
      if (r === "editor") {
        (document.querySelector(".monaco-editor textarea") as HTMLElement | null)?.focus();
      } else {
        (document.querySelector(`[data-region="${r}"]`) as HTMLElement | null)?.focus();
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F6") {
        e.preventDefault();
        const i = Math.max(0, regions.indexOf(focusRegion));
        const step = e.shiftKey ? regions.length - 1 : 1;
        const next = regions[(i + step) % regions.length];
        setFocusRegion(next);
        focusEl(next);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dock, rightTool, focusRegion]);

  // native menu actions
  useEffect(() => {
    const un = listen<string>("menu-action", (e) => {
      switch (e.payload) {
        case "open_folder": void chooseFolder(); break;
        case "save": void save(); break;
        case "search_everywhere": setPaletteOpen(true); break;
        case "toggle_terminal":
        case "new_terminal": setDock("terminal"); break;
        case "settings": setSettingsOpen(true); break;
        case "view_explorer": setSidebar("files"); break;
        case "view_git": setSidebar("git"); break;
        case "view_database": setSidebar("db"); break;
        case "view_extensions": setSidebar("extensions"); break;
        case "new_template": if (summary) setTemplateOpen(true); break;
        case "git_update": gitAction("update"); break;
        case "git_commit": setSidebar("git"); break;
        case "git_push": gitAction("push"); break;
        case "git_new_branch": newBranch(); break;
        case "git_branches": setSidebar("git"); break;
        case "db_new_source": setDbDialog({ open: true, initial: null }); break;
        case "git_pull": gitAction("update"); break;
        case "git_stash": api.gitStash().then(() => showToast("Stashed")).catch((e) => showToast(String(e))); break;
        case "git_log": setSidebar("git"); break;
        case "code_generate":
        case "laravel_generate":
        case "laravel_new_model":
        case "laravel_new_class": if (summary) setTemplateOpen(true); break;
        case "laravel_route_search": setSidebar("routes"); break;
        case "laravel_artisan": setArtisanOpen(true); break;
        case "laravel_phpdoc":
          if (active) {
            api.generateModelPhpdoc(active)
              .then((cs) => applyAndReload(cs))
              .then(() => showToast("Generated model PHPDoc"))
              .catch((e) => showToast(String(e)));
          }
          break;
        case "laravel_missing_views": setSidebar("models"); break;
        case "rename":
        case "refactor_extract_var":
        case "refactor_extract_method":
        case "refactor_inline":
        case "refactor_safe_delete":
          showToast("Refactor: place the cursor / select code in the editor (F2, ⌘⌥V, ⌘⌥M, ⌘⌥N)");
          break;
        case "code_optimize_imports":
        case "code_reformat":
        case "code_move_up":
        case "code_move_down":
        case "code_complete":
        case "code_comment":
          showToast("Editor action — see docs/17 (full v2)");
          break;
        case "about": showToast("Photon IDE 2.16 — native PHP/Laravel IDE"); break;
        case "docs": showToast("See the docs/ folder in the project"); break;
      }
    });
    return () => { un.then((f) => f()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chooseFolder, save, gitAction, newBranch, showToast, summary, active]);

  // Live re-index on external filesystem changes (git checkout, generators,
  // another editor). Keeps navigation/diagnostics correct without re-opening.
  // Open editor buffers are left untouched so unsaved edits are never clobbered.
  useEffect(() => {
    const un = listen<string[]>("fs-changed", async (e) => {
      const paths = e.payload ?? [];
      if (paths.length === 0) return;
      try {
        await Promise.all(paths.map((p) => api.reindexPath(p).catch(() => {})));
        const [f, r] = await Promise.all([api.listFiles(), api.listRoutes()]);
        setFiles(f);
        setRoutes(r);
        if (active && paths.includes(active)) void refreshSymbols(active);
      } catch {
        /* best-effort */
      }
    });
    return () => { un.then((f) => f()); };
  }, [active, refreshSymbols]);

  const activeValue = active ? buffersRef.current[active] ?? "" : "";
  const activeLang = active ? monacoLang(langOf(active)) : "plaintext";

  const ActivityButton = ({ view, glyph, label }: { view: SidebarView; glyph: string; label: string }) => (
    <button
      title={label}
      onClick={() => setSidebar(view)}
      className={`w-9 h-9 flex items-center justify-center rounded-md text-base ${
        sidebar === view ? "text-accent bg-accent/15" : "text-fg-faint hover:text-fg hover:bg-bg-hover"
      }`}
    >
      {glyph}
    </button>
  );

  const sidebarTitle = useMemo(
    () => ({ files: "Explorer", outline: "Structure", routes: "Routes", models: "Eloquent", db: "Database", git: "Source Control", debug: "Debug", extensions: "Extensions", ai: "AI Assistant" }[sidebar]),
    [sidebar]
  );

  // Search Everywhere: Actions + Settings providers.
  const paletteActions = useMemo<PaletteAction[]>(
    () => [
      { label: "Open Folder…", category: "action", run: () => void chooseFolder() },
      { label: "Save File", category: "action", run: () => void save() },
      { label: "New from Template", category: "action", run: () => summary && setTemplateOpen(true) },
      { label: "Run Artisan Command", category: "action", run: () => setArtisanOpen(true) },
      {
        label: "Generate Model PHPDoc",
        category: "action",
        run: () => active && api.generateModelPhpdoc(active).then((cs) => applyAndReload(cs)).catch((e) => showToast(String(e))),
      },
      { label: "Toggle Terminal", category: "action", run: () => toggleTerminal() },
      { label: "Focus Next Panel", category: "action", detail: "F6 · keyboard-first flow", run: () => setFocusRegion((r) => (r === "sidebar" ? "editor" : "sidebar")) },
      { label: "HTTP Client", category: "action", run: () => setDock("http") },
      { label: "Recent Files", category: "action", run: () => setRecentOpen(true) },
      { label: "Git: Commit", category: "action", run: () => setSidebar("git") },
      { label: "Git: Push", category: "action", run: () => gitAction("push") },
      { label: "Git: Update Project", category: "action", run: () => gitAction("update") },
      { label: "New Data Source", category: "action", run: () => setDbDialog({ open: true, initial: null }) },
      { label: "Go to: Explorer", category: "action", run: () => setSidebar("files") },
      { label: "Go to: Source Control", category: "action", run: () => setSidebar("git") },
      { label: "Go to: Database", category: "action", run: () => setSidebar("db") },
      { label: "Go to: Eloquent", category: "action", run: () => setSidebar("models") },
      { label: "Go to: AI Assistant", category: "action", run: () => setSidebar("ai") },
      { label: "Go to: Extensions", category: "action", run: () => setSidebar("extensions") },
      { label: "Settings…", category: "setting", detail: "all preferences", run: () => setSettingsOpen(true) },
      { label: "Setting: Editor font size", category: "setting", run: () => setSettingsOpen(true) },
      { label: "Setting: Auto-save", category: "setting", run: () => setSettingsOpen(true) },
      {
        label: settings.vimMode ? "Vim Mode: Disable" : "Vim Mode: Enable",
        category: "setting",
        detail: "native Vim keybindings",
        run: () => saveSettings({ ...settings, vimMode: !settings.vimMode }),
      },
      { label: "Setting: UI scale", category: "setting", run: () => setSettingsOpen(true) },
      { label: "Setting: AI model / key", category: "setting", run: () => setSettingsOpen(true) },
    ],
    [chooseFolder, save, summary, active, applyAndReload, showToast, gitAction]
  );

  return (
    <div className="h-full flex flex-col">
      <Header
        projectName={summary ? summary.root.split("/").pop() ?? summary.root : null}
        branch={branch}
        summary={summary}
        indexing={indexing}
        onOpenFolder={chooseFolder}
        onSearch={() => setPaletteOpen(true)}
        onSettings={() => setSettingsOpen(true)}
        onGitAction={gitAction}
        onCheckout={checkout}
        onNewBranch={newBranch}
        onToast={showToast}
      />

      <div className="flex-1 flex min-h-0">
        {/* activity bar */}
        <div className="w-12 shrink-0 bg-bg-panel border-r border-border flex flex-col items-center py-2 gap-1">
          <ActivityButton view="files" glyph="▤" label="Explorer" />
          <ActivityButton view="outline" glyph="❮❯" label="Structure" />
          <ActivityButton view="routes" glyph="→" label="Routes" />
          <ActivityButton view="models" glyph="◇" label="Eloquent models" />
          <ActivityButton view="db" glyph="▦" label="Database" />
          <ActivityButton view="git" glyph="⎇" label="Source Control" />
          <ActivityButton view="debug" glyph="🐞" label="Debug (Xdebug)" />
          <ActivityButton view="ai" glyph="✦" label="AI Assistant" />
          <ActivityButton view="extensions" glyph="🧩" label="Extensions" />
          <div className="flex-1" />
          <button
            title="Terminal (⌘`)"
            onClick={() => toggleTerminal()}
            className={`w-9 h-9 flex items-center justify-center rounded-md text-base ${
              dock === "terminal" ? "text-accent bg-accent/15" : "text-fg-faint hover:text-fg hover:bg-bg-hover"
            }`}
          >
            ▱
          </button>
          <button
            title="Community Hub"
            onClick={() => setCommunityOpen(true)}
            className="w-9 h-9 flex items-center justify-center rounded-md text-base text-fg-faint hover:text-fg hover:bg-bg-hover"
          >
            ✦
          </button>
          <button
            title="Settings"
            onClick={() => setSettingsOpen(true)}
            className="w-9 h-9 flex items-center justify-center rounded-md text-base text-fg-faint hover:text-fg hover:bg-bg-hover"
          >
            ⚙
          </button>
        </div>

        {/* sidebar (resizable) */}
        <div
          data-region="sidebar"
          tabIndex={-1}
          onFocusCapture={() => setFocusRegion("sidebar")}
          className={`shrink-0 bg-bg-panel border-r border-border flex flex-col min-h-0 outline-none ${
            focusRegion === "sidebar" ? "ring-1 ring-accent/45 ring-inset" : ""
          }`}
          style={{ width: sidebarWidth }}
        >
          <div className="panel-title flex items-center justify-between">
            <span>{sidebarTitle}</span>
            {summary && sidebar === "files" && (
              <button
                onClick={() => setTemplateOpen(true)}
                title="New from Template (⌘N)"
                className="text-fg-faint hover:text-fg text-base leading-none lowercase"
              >
                +
              </button>
            )}
          </div>
          {/* open projects (multi-root) */}
          {sidebar === "files" && projects.length > 0 && (
            <div className="border-b border-line/60 pb-1">
              {projects.map((p) => (
                <div key={p.label} className="group flex items-center gap-1.5 px-2.5 py-1 text-xs">
                  <span className="text-accent">◆</span>
                  <span className="text-fg-muted truncate flex-1" title={p.path}>{p.label}</span>
                  {p.is_laravel && <span className="text-[#ff7a6e] text-2xs">L</span>}
                  <button
                    onClick={() => void closeProject(p.label)}
                    className="opacity-0 group-hover:opacity-100 text-fg-faint hover:text-danger"
                    title="Close project"
                  >
                    ✕
                  </button>
                </div>
              ))}
              <button
                onClick={chooseFolder}
                className="px-2.5 py-1 text-2xs text-accent hover:underline"
              >
                + Add folder
              </button>
            </div>
          )}
          <div className="flex-1 min-h-0">
            {!summary ? (
              <div className="px-3 py-4 text-fg-faint text-xs leading-relaxed">
                No project open.
                <button onClick={chooseFolder} className="block mt-2 text-accent hover:underline">
                  Open a folder →
                </button>
              </div>
            ) : sidebar === "files" ? (
              <FileTree files={files} activePath={active} onOpen={openFile} />
            ) : sidebar === "outline" ? (
              <OutlinePanel symbols={symbols} onPick={(line) => setReveal({ line, nonce: Date.now() })} />
            ) : sidebar === "routes" ? (
              <RoutesPanel routes={routes} onPick={gotoFileLine} />
            ) : sidebar === "models" ? (
              <ModelsPanel onPick={gotoFileLine} />
            ) : sidebar === "db" ? (
              <DbPanel
                refreshKey={dbRefresh}
                onNewSource={() => setDbDialog({ open: true, initial: null })}
                onEditSource={(ds) => setDbDialog({ open: true, initial: ds })}
                onRunQuery={(connection, sql) => setBottom({ kind: "query", connection, sql })}
                onToast={showToast}
              />
            ) : sidebar === "git" ? (
              <GitSidebar
                onChanged={() => { setGitRefresh((n) => n + 1); void refreshBranch(); }}
                onOpenDiff={openDiff}
                onOpenFile={(f) => void openFile(f)}
                onResolveConflict={(f) => setConflictFile(f)}
                onToast={showToast}
              />
            ) : sidebar === "debug" ? (
              <DebugPanel
                breakpoints={breakpoints}
                onLocate={(file, line) => void openFile(file, line)}
                onToggleBreakpoint={toggleBreakpoint}
              />
            ) : sidebar === "ai" ? (
              <AiPanel getContext={getAiContext} onOpenSettings={() => setSettingsOpen(true)} pending={aiPending} />
            ) : (
              <ExtensionsPanel />
            )}
          </div>
        </div>

        <ResizeHandle
          dir="x"
          onDelta={(dx) => setSidebarWidth((w) => Math.max(180, Math.min(560, w + dx)))}
        />

        {/* main area */}
        <div
          data-region="editor"
          onFocusCapture={() => setFocusRegion("editor")}
          className={`flex-1 flex flex-col min-w-0 relative outline-none ${
            focusRegion === "editor" ? "ring-1 ring-accent/40 ring-inset" : ""
          }`}
        >
          {renameTarget && (
            <InlineRename
              oldName={renameTarget}
              onClose={() => setRenameTarget(null)}
              onApplied={(n) => { void onRenameApplied(); showToast(`Renamed across ${n} file${n === 1 ? "" : "s"}`); }}
            />
          )}
          {sidebar === "git" ? (
            conflictFile ? (
              <ConflictCenter
                file={conflictFile}
                onClose={() => setConflictFile(null)}
                onResolved={() => {
                  setConflictFile(null);
                  setGitRefresh((n) => n + 1);
                  void refreshBranch();
                }}
              />
            ) : (
              <>
                <div className="h-9 shrink-0 flex items-center gap-1 px-2 bg-bg-panel border-b border-border text-sm">
                  <button
                    onClick={() => setGitView("graph")}
                    className={`px-2 py-0.5 rounded text-xs ${gitView === "graph" ? "bg-accent/20 text-accent" : "text-fg-faint hover:text-fg"}`}
                  >
                    Commit Graph
                  </button>
                  <button
                    onClick={() => setGitView("insights")}
                    className={`px-2 py-0.5 rounded text-xs ${gitView === "insights" ? "bg-accent/20 text-accent" : "text-fg-faint hover:text-fg"}`}
                  >
                    Insights
                  </button>
                  {gitView === "graph" && (
                    <span className="text-fg-faint text-xs ml-2 truncate">
                      — drag a branch onto a commit (move/reset) or another branch (merge); drag a commit onto a branch (cherry-pick)
                    </span>
                  )}
                </div>
                {gitView === "graph" ? (
                  <CommitGraph
                    refreshKey={gitRefresh}
                    currentBranch={branch}
                    onChanged={() => { setGitRefresh((n) => n + 1); void refreshBranch(); }}
                    onRebaseFrom={(h) => setRebaseBase(h)}
                  />
                ) : (
                  <InsightsPanel refreshKey={gitRefresh} />
                )}
              </>
            )
          ) : (
            <>
              {active && (
                <div className="relative shrink-0">
                  <div className="h-6 flex items-center gap-1 px-3 surface-1 border-b border-line/50 text-2xs text-fg-faint overflow-x-auto whitespace-nowrap">
                    {active.split("/").map((seg, i, arr) => (
                      <span key={i} className="flex items-center gap-1">
                        {i > 0 && <span className="text-fg-faint/50">›</span>}
                        <button
                          className={`hover:text-fg ${i === arr.length - 1 ? "text-fg-muted" : ""}`}
                          onClick={() =>
                            setCrumb((c) => {
                              const p = i === arr.length - 1 ? arr.slice(0, i).join("/") : arr.slice(0, i + 1).join("/");
                              return c === p ? null : p;
                            })
                          }
                        >
                          {seg}
                        </button>
                      </span>
                    ))}
                  </div>
                  {crumb && (
                    <div
                      className="absolute left-3 top-full mt-0.5 z-50 w-60 max-h-72 overflow-auto rounded-lg border border-border bg-bg-panel shadow-2xl p-1 text-xs"
                      onMouseLeave={() => setCrumb(null)}
                    >
                      {crumbChildren.length === 0 && <div className="px-2 py-1 text-fg-faint">empty</div>}
                      {crumbChildren.map((c) => (
                        <button
                          key={c.full}
                          className="w-full text-left px-2 py-1 rounded hover:bg-bg-hover flex items-center gap-1.5"
                          onClick={() => {
                            if (c.dir) setCrumb(c.full);
                            else { void openFile(c.full); setCrumb(null); }
                          }}
                        >
                          <span className="text-fg-faint">{c.dir ? "▸" : "·"}</span>
                          <span className="truncate text-fg-muted">{c.name}</span>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              )}
              {tabs.length > 0 && (
                <div className="h-10 shrink-0 flex items-center gap-1.5 px-2 surface-2 border-b border-line overflow-x-auto">
                  {tabs.map((t) => {
                    const base = t.path.split("/").pop();
                    const on = active === t.path;
                    return (
                      <div
                        key={t.path}
                        onClick={() => { setActive(t.path); void refreshSymbols(t.path); }}
                        className={`group relative flex items-center gap-2 pl-3 pr-2 h-7 rounded-md cursor-default text-sm whitespace-nowrap transition-all duration-180 ${
                          on
                            ? "bg-surface-0 text-fg shadow-e1 border border-line"
                            : "text-fg-muted hover:bg-white/[0.05] border border-transparent"
                        }`}
                      >
                        {on && (
                          <span className="absolute -top-px left-2 right-2 h-[2px] rounded-full bg-accent" />
                        )}
                        <span>{base}</span>
                        {dirty[t.path] ? (
                          <span className="text-warn text-xs">●</span>
                        ) : null}
                        <span
                          onClick={(e) => { e.stopPropagation(); closeTab(t.path); }}
                          className="opacity-0 group-hover:opacity-100 text-fg-faint hover:text-fg transition-opacity"
                        >
                          ✕
                        </span>
                      </div>
                    );
                  })}
                  {container && (
                    <button
                      onClick={() => setDock("terminal")}
                      title={
                        container === "devcontainer"
                          ? "Dev Container detected (.devcontainer) — open a shell"
                          : container === "compose"
                          ? "Docker Compose detected — open a shell"
                          : "Dockerfile detected — open a shell"
                      }
                      className="ml-auto shrink-0 flex items-center gap-1.5 h-7 px-2.5 rounded-md text-xs text-[#4aa3df] bg-[#4aa3df]/10 hover:bg-[#4aa3df]/20 border border-[#4aa3df]/20"
                    >
                      <span>▣</span>
                      <span className="font-medium">Run in Container</span>
                    </button>
                  )}
                </div>
              )}
              <EditorPane
                path={active}
                value={activeValue}
                language={activeLang}
                reveal={reveal}
                settings={settings}
                lintKey={lintKey}
                onChange={onEditorChange}
                onSave={save}
                onRequestRename={setRenameTarget}
                onFindUsages={findUsages}
                onExtractVariable={extractVariable}
                onExtractMethod={extractMethod}
                onInlineVariable={inlineVariable}
                onSafeDelete={safeDelete}
                onChangeSignature={changeSignature}
                onMoveClass={moveClass}
                onLocalHistory={() => { if (active) setHistoryPath(active); }}
                onToggleBreakpoint={(line) => { if (active) toggleBreakpoint(active, line); }}
                onConditionalBreakpoint={(line) => { if (active) conditionalBreakpoint(active, line); }}
                breakpointLines={active ? breakpoints.filter((b) => b.path === active).map((b) => b.line) : []}
                onCmdClick={cmdClick}
                onResolveKey={resolveKey}
                onResolveBinding={resolveBinding}
                onGotoDefinition={gotoDefinition}
                onGotoImplementation={gotoImplementation}
                onGotoType={gotoType}
                onAiAsk={askAi}
                debugActive={debug.active}
                debugLine={debug.file === active ? debug.line : null}
                debugInline={debug.file === active ? debug.inline : null}
                testLines={testLines}
                onRunTest={runTestAt}
              />
            </>
          )}

          {/* bottom tool dock (resizable) */}
          {bottom && (
            <>
              <ResizeHandle
                dir="y"
                onDelta={(dy) => setBottomHeight((h) => Math.max(120, Math.min(700, h - dy)))}
              />
            <div
              className="shrink-0 border-t border-border bg-bg-panel flex flex-col"
              style={{ height: bottomHeight }}
            >
              {bottom.kind === "usages" ? (
                <UsagesPanel name={bottom.name} usages={bottom.list} onClose={() => setBottom(null)} onPick={gotoFileLine} />
              ) : bottom.kind === "query" ? (
                <div className="h-full flex flex-col">
                  <div className="flex items-center justify-between px-2 py-1 border-b border-border">
                    <span className="text-xs text-fg-muted">Query Console</span>
                    <button onClick={() => setBottom(null)} className="text-fg-faint hover:text-fg text-xs">✕</button>
                  </div>
                  <div className="flex-1 min-h-0">
                    <QueryRunner connection={bottom.connection} initialSql={bottom.sql} />
                  </div>
                </div>
              ) : (
                <div className="h-full flex flex-col">
                  <div className="flex items-center justify-between px-2 py-1 border-b border-line">
                    <span className="text-xs text-fg-muted">Diff — {bottom.file} (side-by-side)</span>
                    <button onClick={() => setBottom(null)} className="text-fg-faint hover:text-fg text-xs">✕</button>
                  </div>
                  <DiffViewer original={bottom.original} modified={bottom.modified} file={bottom.file} />
                </div>
              )}
            </div>
            </>
          )}

          {/* tabbed bottom dock (Terminal · Xdebug · HTTP) */}
          {dock && (
            <>
              <ResizeHandle
                dir="y"
                onDelta={(dy) => setTerminalHeight((h) => Math.max(120, Math.min(700, h - dy)))}
              />
              <div
                data-region="dock"
                tabIndex={-1}
                onFocusCapture={() => setFocusRegion("dock")}
                className={`shrink-0 border-t border-border bg-bg-panel flex flex-col outline-none ${
                  focusRegion === "dock" ? "ring-1 ring-accent/45 ring-inset" : ""
                }`}
                style={{ height: terminalHeight }}
              >
                <div className="flex items-center gap-0.5 px-2 h-7 shrink-0 border-b border-line text-2xs">
                  {([
                    ["terminal", "▱ Terminal"],
                    ["debug", "🐞 Xdebug"],
                    ["http", "⇅ HTTP"],
                  ] as [DockTab, string][]).map(([id, label]) => (
                    <button
                      key={id}
                      onClick={() => setDock(id)}
                      className={`px-2.5 py-1 rounded-t ${
                        dock === id
                          ? "text-fg bg-bg border-b-[1.5px] border-accent -mb-px"
                          : "text-fg-faint hover:text-fg"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                  <button
                    onClick={() => setDock(null)}
                    className="ml-auto text-fg-faint hover:text-fg px-1.5"
                    title="Close panel"
                  >
                    ✕
                  </button>
                </div>
                <div className="flex-1 min-h-0 relative">
                  {/* Terminal stays mounted to preserve the PTY session. */}
                  <div className={dock === "terminal" ? "absolute inset-0" : "hidden"}>
                    <TerminalDock cwd={summary?.root ?? null} onClose={() => setDock(null)} />
                  </div>
                  {dock === "debug" && (
                    <DebugPanel
                      breakpoints={breakpoints}
                      onLocate={(file, line) => void openFile(file, line)}
                      onToggleBreakpoint={toggleBreakpoint}
                    />
                  )}
                  {dock === "http" && <HttpClient routes={routes} />}
                </div>
              </div>
            </>
          )}
        </div>

        {/* right-docked tool panel */}
        {rightTool && (
          <>
            <ResizeHandle
              dir="x"
              onDelta={(dx) => setRightWidth((w) => Math.max(220, Math.min(560, w - dx)))}
            />
            <div
              data-region="right"
              tabIndex={-1}
              onFocusCapture={() => setFocusRegion("right")}
              className={`shrink-0 bg-bg-panel border-l border-border flex flex-col min-h-0 outline-none ${
                focusRegion === "right" ? "ring-1 ring-accent/45 ring-inset" : ""
              }`}
              style={{ width: rightWidth }}
            >
              <div className="h-8 shrink-0 flex items-center justify-between px-3 border-b border-line text-2xs uppercase tracking-wider text-fg-faint">
                <span>{rightTool === "arch" ? "Architecture Map" : "Structure"}</span>
                <button onClick={() => setRightTool(null)} className="hover:text-fg">✕</button>
              </div>
              <div className="flex-1 min-h-0">
                {rightTool === "arch" ? (
                  <ArchMap
                    code={active ? activeValue : null}
                    fileName={active}
                    onPick={(name) => void findUsages(name)}
                  />
                ) : (
                  <OutlinePanel symbols={symbols} onPick={(line) => setReveal({ line, nonce: Date.now() })} />
                )}
              </div>
            </div>
          </>
        )}

        {/* right activity strip */}
        <div className="w-10 shrink-0 bg-bg-panel border-l border-border flex flex-col items-center py-2 gap-1">
          {([
            ["arch", "◈", "Architecture Map"],
            ["outline", "≡", "Structure"],
          ] as ["arch" | "outline", string, string][]).map(([id, glyph, label]) => (
            <button
              key={id}
              title={label}
              onClick={() => setRightTool((t) => (t === id ? null : id))}
              className={`w-8 h-8 flex items-center justify-center rounded-md text-base ${
                rightTool === id ? "text-accent bg-accent/15" : "text-fg-faint hover:text-fg hover:bg-bg-hover"
              }`}
            >
              {glyph}
            </button>
          ))}
        </div>
      </div>

      <StatusBar
        summary={summary}
        activePath={active}
        dirty={active ? !!dirty[active] : false}
        indexing={indexing}
        branch={branch}
        onToggleTerminal={() => toggleTerminal()}
        onGitChanged={() => { setGitRefresh((n) => n + 1); void refreshBranch(); }}
        onToast={showToast}
      />

      <SearchEverywhere
        open={paletteOpen}
        actions={paletteActions}
        onClose={() => setPaletteOpen(false)}
        onPick={gotoFileLine}
      />
      {recentOpen && (
        <RecentPopup
          files={recentFiles}
          locations={recentLocations}
          onClose={() => setRecentOpen(false)}
          onPick={gotoFileLine}
        />
      )}

      {settingsOpen && <SettingsDialog current={settings} onClose={() => setSettingsOpen(false)} />}
      {communityOpen && <CommunityHub onClose={() => setCommunityOpen(false)} />}
      {artisanOpen && <ArtisanRunner onClose={() => setArtisanOpen(false)} />}
      {rebaseBase && (
        <RebaseModal
          base={rebaseBase}
          onClose={() => setRebaseBase(null)}
          onDone={(msg) => {
            setRebaseBase(null);
            setGitRefresh((n) => n + 1);
            void refreshBranch();
            showToast(msg);
          }}
        />
      )}
      {historyPath && (
        <HistoryPopup
          path={historyPath}
          onClose={() => setHistoryPath(null)}
          onDiff={async (ts) => {
            try {
              const snap = await api.historyGet(historyPath, ts);
              setBottom({ kind: "diff", file: historyPath, original: snap, modified: buffersRef.current[historyPath] ?? "" });
              setHistoryPath(null);
            } catch (e) {
              showToast(String(e));
            }
          }}
          onRestore={async (ts) => {
            try {
              const snap = await api.historyGet(historyPath, ts);
              await api.saveFile(historyPath, snap);
              buffersRef.current[historyPath] = snap;
              setLintKey((n) => n + 1);
              setHistoryPath(null);
              showToast("Restored from history");
            } catch (e) {
              showToast(String(e));
            }
          }}
        />
      )}
      {templateOpen && (
        <TemplateDialog
          onClose={() => setTemplateOpen(false)}
          onCreated={async (path) => {
            try {
              setFiles(await api.listFiles());
            } catch {
              /* ignore */
            }
            void openFile(path);
            showToast(`Created ${path}`);
          }}
        />
      )}
      {dbDialog.open && (
        <DbConnectionDialog
          initial={dbDialog.initial}
          onClose={() => setDbDialog({ open: false, initial: null })}
          onSaved={() => { setDbRefresh((n) => n + 1); setDbDialog({ open: false, initial: null }); }}
        />
      )}
      {prompt && (
        <div className="fixed inset-0 z-50 flex items-start justify-center pt-[20vh] bg-black/40" onClick={() => setPrompt(null)}>
          <div className="pop-in w-96 bg-bg-panel border border-border rounded-lg shadow-2xl p-3" onClick={(e) => e.stopPropagation()}>
            <div className="text-sm text-fg-muted mb-2">{prompt.label}</div>
            <input
              autoFocus
              defaultValue={prompt.value}
              onKeyDown={(e) => {
                if (e.key === "Enter") { prompt.onOk((e.target as HTMLInputElement).value); setPrompt(null); }
                if (e.key === "Escape") setPrompt(null);
              }}
              className="w-full bg-bg-elevated border border-border rounded px-2 py-1.5 text-sm outline-none focus:border-accent"
            />
          </div>
        </div>
      )}

      {usagesPopup && (
        <UsagesPopup
          result={usagesPopup.result}
          x={usagesPopup.x}
          y={usagesPopup.y}
          onClose={() => setUsagesPopup(null)}
          onPick={gotoFileLine}
        />
      )}

      {toasts.length > 0 && (
        <div className="fixed bottom-8 left-1/2 -translate-x-1/2 z-50 flex flex-col items-center gap-2">
          {toasts.map((t) => {
            const color = t.kind === "error" ? "#f85149" : t.kind === "success" ? "#3fb950" : "#3574f0";
            const icon = t.kind === "error" ? "✕" : t.kind === "success" ? "✓" : "›";
            return (
              <div
                key={t.id}
                onClick={() => setToasts((l) => l.filter((x) => x.id !== t.id))}
                className="pop-in flex items-center gap-2 bg-bg-elevated border border-border rounded-lg pl-3 pr-4 py-2 text-sm text-fg shadow-2xl cursor-default"
                style={{ borderLeft: `3px solid ${color}` }}
                title="Dismiss"
              >
                <span className="font-semibold leading-none" style={{ color }}>{icon}</span>
                <span className="max-w-[56vw] truncate">{t.msg}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
