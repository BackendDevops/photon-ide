//! Git integration (Pillar 4, v1) — `docs/02-module-design.md` §git.
//!
//! v1 drives the system `git` binary via `std::process::Command` and parses
//! porcelain output. The design doc prefers gitoxide (`gix`) with a CLI
//! fallback; this is the CLI path — robust and complete — and the gix backend
//! is a drop-in optimization behind the same command surface later.
//!
//! Requires `git` on PATH. Verified against a real repo on the host.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub path: String,
    /// Two-letter porcelain code, e.g. " M", "A ", "??".
    pub status: String,
    pub staged: bool,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    pub branch: String,
    pub ahead: i32,
    pub behind: i32,
    pub files: Vec<GitFileStatus>,
    pub clean: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    pub hash: String,
    pub short: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub current: bool,
}

fn run(root: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn is_repo(root: &str) -> bool {
    Path::new(root).join(".git").exists()
        || run(root, &["rev-parse", "--is-inside-work-tree"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false)
}

fn status_label(code: &str) -> &'static str {
    match code.trim() {
        "M" => "modified",
        "A" => "added",
        "D" => "deleted",
        "R" => "renamed",
        "C" => "copied",
        "U" => "conflict",
        "??" => "untracked",
        _ => "changed",
    }
}

pub fn status(root: &str) -> Result<GitStatus, String> {
    let raw = run(root, &["status", "--porcelain=v1", "--branch"])?;
    let mut branch = String::from("(detached)");
    let mut ahead = 0;
    let mut behind = 0;
    let mut files = Vec::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            // e.g. "main...origin/main [ahead 1, behind 2]"
            branch = rest
                .split(['.', ' '])
                .next()
                .unwrap_or("")
                .to_string();
            if let Some(a) = rest.find("ahead ") {
                ahead = rest[a + 6..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
            }
            if let Some(b) = rest.find("behind ") {
                behind = rest[b + 7..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
            }
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..].to_string();
        let index_ch = code.chars().next().unwrap_or(' ');
        let work_ch = code.chars().nth(1).unwrap_or(' ');
        let staged = index_ch != ' ' && index_ch != '?';
        // Prefer the index code when staged, else the worktree code.
        let effective = if staged {
            index_ch.to_string()
        } else if code == "??" {
            "??".to_string()
        } else {
            work_ch.to_string()
        };
        files.push(GitFileStatus {
            path,
            status: code.to_string(),
            staged,
            label: status_label(&effective).to_string(),
        });
    }

    Ok(GitStatus {
        clean: files.is_empty(),
        branch,
        ahead,
        behind,
        files,
    })
}

pub fn stage(root: &str, paths: &[String]) -> Result<(), String> {
    let mut args = vec!["add", "--"];
    let owned: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    args.extend(owned);
    run(root, &args).map(|_| ())
}

pub fn unstage(root: &str, paths: &[String]) -> Result<(), String> {
    let mut args = vec!["restore", "--staged", "--"];
    let owned: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    args.extend(owned);
    run(root, &args).map(|_| ())
}

pub fn commit(root: &str, message: &str) -> Result<String, String> {
    run(root, &["commit", "-m", message])
}

pub fn branches(root: &str) -> Result<Vec<Branch>, String> {
    let raw = run(root, &["branch", "--list"])?;
    Ok(raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let current = l.starts_with('*');
            Branch {
                name: l.trim_start_matches('*').trim().to_string(),
                current,
            }
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub current: bool,
    pub remote: bool,
    /// Upstream tracking ref (e.g. origin/main), if any.
    pub upstream: Option<String>,
    pub ahead: i32,
    pub behind: i32,
    /// Relative last-commit time (e.g. "3 days ago").
    pub last_commit: String,
}

/// Rich branch list for the JetBrains-style branch popover, with per-branch
/// upstream + ahead/behind counts (the ↑/↓ indicators).
pub fn branches_detailed(root: &str) -> Result<Vec<BranchInfo>, String> {
    // short\x1f upstream\x1f track\x1f date\x1f HEAD\x1f fullref
    let fmt = "%(refname:short)\x1f%(upstream:short)\x1f%(upstream:track)\x1f%(committerdate:relative)\x1f%(HEAD)\x1f%(refname)";
    let raw = run(
        root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            &format!("--format={fmt}"),
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let p: Vec<&str> = line.split('\u{1f}').collect();
        if p.is_empty() || p[0].is_empty() {
            continue;
        }
        let name = p[0].to_string();
        if name.ends_with("/HEAD") {
            continue;
        }
        let upstream = p.get(1).filter(|s| !s.is_empty()).map(|s| s.to_string());
        let track = p.get(2).copied().unwrap_or("");
        let (mut ahead, mut behind) = (0, 0);
        if let Some(a) = track.find("ahead ") {
            ahead = track[a + 6..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
        }
        if let Some(b) = track.find("behind ") {
            behind = track[b + 7..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
        }
        let fullref = p.get(5).copied().unwrap_or("");
        let remote = fullref.starts_with("refs/remotes/");
        out.push(BranchInfo {
            current: p.get(4).map(|s| *s == "*").unwrap_or(false),
            remote,
            upstream,
            ahead,
            behind,
            last_commit: p.get(3).copied().unwrap_or("").to_string(),
            name,
        });
    }
    Ok(out)
}

/// `git pull --ff-only` plus a fetch first (the "Update Project" action).
pub fn update_project(root: &str) -> Result<String, String> {
    let _ = run(root, &["fetch", "--all", "--prune"]);
    run(root, &["pull", "--ff-only"])
}

pub fn cherry_pick(root: &str, hash: &str) -> Result<String, String> {
    run(root, &["cherry-pick", hash])
}

/// Unified diff between two refs (branch comparison): `base...head`.
pub fn compare(root: &str, base: &str, head: &str) -> Result<String, String> {
    run(root, &["diff", &format!("{base}...{head}")])
}

/// Files currently in merge conflict.
pub fn conflicts(root: &str) -> Result<Vec<String>, String> {
    let raw = run(root, &["diff", "--name-only", "--diff-filter=U"])?;
    Ok(raw.lines().filter(|l| !l.trim().is_empty()).map(|s| s.to_string()).collect())
}

/// Resolve a conflict by taking one side, then stage it. side = "ours" | "theirs".
pub fn resolve_conflict(root: &str, file: &str, side: &str) -> Result<String, String> {
    let flag = if side == "theirs" { "--theirs" } else { "--ours" };
    run(root, &["checkout", flag, "--", file])?;
    run(root, &["add", "--", file])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictVersions {
    /// Common ancestor (index stage 1); empty for add/add conflicts.
    pub base: String,
    /// Our side (stage 2 — current branch).
    pub ours: String,
    /// Their side (stage 3 — incoming).
    pub theirs: String,
    /// Working-tree content with conflict markers.
    pub working: String,
    pub ours_label: String,
    pub theirs_label: String,
}

/// The three merge stages plus the marked-up working copy for a conflicted file.
pub fn conflict_versions(root: &str, file: &str) -> Result<ConflictVersions, String> {
    let stage = |n: u8| run(root, &["show", &format!(":{n}:{file}")]).unwrap_or_default();
    let working =
        std::fs::read_to_string(std::path::Path::new(root).join(file)).unwrap_or_default();
    let ours_label = run(root, &["symbolic-ref", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ours".into());
    // Best-effort name for the incoming side (branch/commit being merged).
    let theirs_label = run(root, &["name-rev", "--name-only", "MERGE_HEAD"])
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "incoming".into());
    Ok(ConflictVersions {
        base: stage(1),
        ours: stage(2),
        theirs: stage(3),
        working,
        ours_label,
        theirs_label,
    })
}

/// Write a fully-resolved file back to the working tree and stage it (marks the
/// conflict resolved). Used by the 3-way conflict resolution center.
pub fn resolve_content(root: &str, file: &str, content: &str) -> Result<String, String> {
    let full = std::path::Path::new(root).join(file);
    std::fs::write(&full, content).map_err(|e| format!("write {file}: {e}"))?;
    run(root, &["add", "--", file])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSides {
    /// The committed (HEAD) version; empty for a newly-added file.
    pub original: String,
    /// The current working-tree version.
    pub modified: String,
}

/// Both sides of a file's changes, for a side-by-side diff viewer.
pub fn diff_sides(root: &str, file: &str) -> Result<DiffSides, String> {
    let original = run(root, &["show", &format!("HEAD:{file}")]).unwrap_or_default();
    let modified = std::fs::read_to_string(std::path::Path::new(root).join(file)).unwrap_or_default();
    Ok(DiffSides { original, modified })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    pub line: u32,
    pub short: String,
    pub author: String,
    pub summary: String,
}

/// `git blame --line-porcelain` parsed into per-line authorship.
pub fn blame(root: &str, file: &str) -> Result<Vec<BlameLine>, String> {
    let raw = run(root, &["blame", "--line-porcelain", "--", file])?;
    let mut out = Vec::new();
    let mut meta: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    let mut cur_hash = String::new();
    let mut cur_author = String::new();
    let mut cur_summary = String::new();
    let mut line_no = 0u32;

    for line in raw.lines() {
        if line.starts_with('\t') {
            // content line → emit
            line_no += 1;
            let short: String = cur_hash.chars().take(7).collect();
            meta.entry(cur_hash.clone())
                .or_insert((cur_author.clone(), cur_summary.clone()));
            out.push(BlameLine {
                line: line_no,
                short,
                author: cur_author.clone(),
                summary: cur_summary.clone(),
            });
        } else if let Some(rest) = line.strip_prefix("author ") {
            cur_author = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("summary ") {
            cur_summary = rest.to_string();
        } else {
            // a header line "<40-hex> orig final num"
            let first = line.split(' ').next().unwrap_or("");
            if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
                cur_hash = first.to_string();
                if let Some((a, s)) = meta.get(&cur_hash) {
                    cur_author = a.clone();
                    cur_summary = s.clone();
                }
            }
        }
    }
    Ok(out)
}

pub fn checkout(root: &str, branch: &str) -> Result<String, String> {
    run(root, &["checkout", branch])
}

/// Merge `branch` into the current HEAD (no editor).
pub fn merge(root: &str, branch: &str) -> Result<String, String> {
    run(root, &["merge", "--no-edit", branch])
}

/// Force-move a branch pointer to `target` (drag a non-current branch onto a
/// commit). Refuses the checked-out branch (use reset for that).
pub fn branch_force(root: &str, name: &str, target: &str) -> Result<String, String> {
    run(root, &["branch", "-f", name, target])
}

fn parse_remote(remote: &str) -> Option<(String, String)> {
    // scp-like: git@host:owner/repo(.git)
    if let Some(rest) = remote.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return Some((host.to_string(), path.to_string()));
    }
    for pre in ["https://", "http://", "ssh://"] {
        if let Some(rest) = remote.strip_prefix(pre) {
            let rest = rest.strip_prefix("git@").unwrap_or(rest);
            let (host, path) = rest.split_once('/')?;
            return Some((host.to_string(), path.to_string()));
        }
    }
    None
}

/// Build the host's "create pull/merge request" URL for the current branch.
pub fn pr_url(root: &str) -> Result<String, String> {
    let remote = run(root, &["config", "--get", "remote.origin.url"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if remote.is_empty() {
        return Err("No 'origin' remote configured.".into());
    }
    let branch = run(root, &["symbolic-ref", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if branch.is_empty() {
        return Err("Detached HEAD — checkout a branch first.".into());
    }
    let (host, path) = parse_remote(&remote).ok_or("Unrecognized remote URL.")?;
    let path = path.trim_end_matches(".git");
    let url = if host.contains("github") {
        format!("https://{host}/{path}/compare/{branch}?expand=1")
    } else if host.contains("gitlab") {
        format!("https://{host}/{path}/-/merge_requests/new?merge_request%5Bsource_branch%5D={branch}")
    } else if host.contains("bitbucket") {
        format!("https://{host}/{path}/pull-requests/new?source={branch}")
    } else {
        format!("https://{host}/{path}")
    };
    Ok(url)
}

/// Commits in `base..HEAD`, oldest-first — the set an interactive rebase edits.
pub fn rebase_list(root: &str, base: &str) -> Result<Vec<GitCommit>, String> {
    let fmt = "%H%x1f%h%x1f%an%x1f%ad%x1f%s";
    let raw = run(
        root,
        &[
            "log",
            "--reverse",
            "--date=short",
            &format!("--pretty=format:{fmt}"),
            &format!("{base}..HEAD"),
        ],
    )?;
    Ok(raw
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split('\u{1f}').collect();
            (p.len() == 5).then(|| GitCommit {
                hash: p[0].into(),
                short: p[1].into(),
                author: p[2].into(),
                date: p[3].into(),
                subject: p[4].into(),
            })
        })
        .collect())
}

/// Run a non-interactive `git rebase -i base` by feeding a pre-built todo list
/// through a scripted sequence editor. `GIT_EDITOR=true` accepts default
/// (squash/fixup) commit messages. On failure the rebase is aborted so the
/// working tree is never left mid-rebase. (docs/16 §3)
pub fn rebase_interactive(root: &str, base: &str, todo: &str) -> Result<String, String> {
    let mut path = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.push(format!("photon-rebase-{}-{stamp}.txt", std::process::id()));
    std::fs::write(&path, todo).map_err(|e| format!("temp todo: {e}"))?;
    // Single-quote the path for the shell; escape embedded quotes.
    let quoted = format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("-c")
        .arg(format!("sequence.editor=cp {quoted}"))
        .env("GIT_EDITOR", "true")
        .args(["rebase", "-i", "--autostash", base])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    let _ = std::fs::remove_file(&path);
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        let _ = run(root, &["rebase", "--abort"]);
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LineStatus {
    pub added: Vec<u32>,
    pub modified: Vec<u32>,
    /// Lines after which content was deleted.
    pub deleted: Vec<u32>,
}

/// Per-line git status for the editor gutter, parsed from `git diff -U0`.
pub fn line_status(root: &str, file: &str) -> Result<LineStatus, String> {
    let raw = run(root, &["diff", "-U0", "--no-color", "--", file]).unwrap_or_default();
    let mut out = LineStatus::default();
    for line in raw.lines() {
        // @@ -a,b +c,d @@
        if !line.starts_with("@@") {
            continue;
        }
        let plus = match line.find('+') {
            Some(p) => &line[p + 1..],
            None => continue,
        };
        let seg = plus.split(|c| c == ' ' || c == ',').collect::<Vec<_>>();
        let c: u32 = seg.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let d: u32 = if plus.contains(',') {
            seg.get(1).and_then(|s| s.parse().ok()).unwrap_or(1)
        } else {
            1
        };
        // old count b
        let minus = line.find('-').map(|m| &line[m + 1..]).unwrap_or("");
        let b: u32 = if minus.contains(',') {
            minus.split(',').nth(1).and_then(|s| s.split(' ').next()).and_then(|s| s.parse().ok()).unwrap_or(1)
        } else {
            1
        };
        if d == 0 {
            out.deleted.push(c.max(1));
        } else if b == 0 {
            for l in c..c + d {
                out.added.push(l);
            }
        } else {
            for l in c..c + d {
                out.modified.push(l);
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insights {
    pub total_commits: u32,
    /// (author, commit count) — top contributors.
    pub contributors: Vec<(String, u32)>,
    /// (YYYY-MM-DD, commit count) — recent daily activity, oldest first.
    pub activity: Vec<(String, u32)>,
    /// (path, times touched) — most-changed files in recent history.
    pub files: Vec<(String, u32)>,
}

fn count_lines(raw: &str) -> Vec<(String, u32)> {
    let mut map: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for l in raw.lines() {
        let k = l.trim();
        if k.is_empty() {
            continue;
        }
        *map.entry(k.to_string()).or_insert(0) += 1;
    }
    let mut v: Vec<(String, u32)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// Repository insights: contributors, recent activity, hot files. (docs/16 §8)
pub fn insights(root: &str) -> Result<Insights, String> {
    let total = run(root, &["rev-list", "--count", "HEAD"])
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    let mut contributors = count_lines(
        &run(root, &["log", "--no-merges", "-n", "2000", "--pretty=%an"]).unwrap_or_default(),
    );
    contributors.truncate(12);

    // Daily activity → keep the most recent 14 days, oldest-first for charting.
    let mut activity = count_lines(
        &run(root, &["log", "-n", "2000", "--date=short", "--pretty=%ad"]).unwrap_or_default(),
    );
    activity.sort_by(|a, b| b.0.cmp(&a.0)); // newest date first
    activity.truncate(14);
    activity.reverse();

    let mut files = count_lines(
        &run(root, &["log", "-n", "400", "--name-only", "--pretty=format:"]).unwrap_or_default(),
    );
    files.truncate(15);

    Ok(Insights {
        total_commits: total,
        contributors,
        activity,
        files,
    })
}

pub fn create_branch(root: &str, name: &str) -> Result<String, String> {
    run(root, &["checkout", "-b", name])
}

pub fn diff(root: &str, file: &str) -> Result<String, String> {
    // Combined unstaged + staged diff for the file.
    let unstaged = run(root, &["diff", "--", file]).unwrap_or_default();
    let staged = run(root, &["diff", "--cached", "--", file]).unwrap_or_default();
    Ok(format!("{staged}{unstaged}"))
}

pub fn log(root: &str, limit: u32) -> Result<Vec<GitCommit>, String> {
    // Use unit-separator so subjects with any char survive parsing.
    let fmt = "%H%x1f%h%x1f%an%x1f%ad%x1f%s";
    let raw = run(
        root,
        &[
            "log",
            &format!("-{limit}"),
            "--date=short",
            &format!("--pretty=format:{fmt}"),
        ],
    )?;
    Ok(raw
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split('\u{1f}').collect();
            if p.len() == 5 {
                Some(GitCommit {
                    hash: p[0].into(),
                    short: p[1].into(),
                    author: p[2].into(),
                    date: p[3].into(),
                    subject: p[4].into(),
                })
            } else {
                None
            }
        })
        .collect())
}

pub fn push(root: &str) -> Result<String, String> {
    run(root, &["push"])
}

pub fn pull(root: &str) -> Result<String, String> {
    run(root, &["pull", "--ff-only"])
}

/// Discard working-tree changes to a file (rollback): restore from HEAD/index.
pub fn discard(root: &str, file: &str) -> Result<String, String> {
    // Unstage if staged, then restore the working copy.
    let _ = run(root, &["restore", "--staged", "--", file]);
    run(root, &["checkout", "--", file]).or_else(|_| run(root, &["restore", "--", file]))
}

pub fn stash(root: &str) -> Result<String, String> {
    run(root, &["stash", "push", "-u"])
}

pub fn stash_pop(root: &str) -> Result<String, String> {
    run(root, &["stash", "pop"])
}

/// Run git with `input` piped to stdin (for `git apply -`). Closes stdin before
/// reading output to avoid large-patch deadlocks.
fn run_stdin(root: &str, args: &[&str], input: &str) -> Result<String, String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| format!("git stdin: {e}"))?;
    } // stdin dropped here → EOF
    let out = child
        .wait_with_output()
        .map_err(|e| format!("git wait: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Amend the previous commit. An empty message keeps the existing one.
pub fn amend(root: &str, message: &str) -> Result<String, String> {
    if message.trim().is_empty() {
        run(root, &["commit", "--amend", "--no-edit"])
    } else {
        run(root, &["commit", "--amend", "-m", message])
    }
}

/// Move HEAD to `target` with `mode` = "soft" | "mixed" | "hard".
pub fn reset(root: &str, target: &str, mode: &str) -> Result<String, String> {
    let flag = match mode {
        "soft" => "--soft",
        "hard" => "--hard",
        _ => "--mixed",
    };
    run(root, &["reset", flag, target])
}

/// Create a new commit that undoes `hash` (no editor).
pub fn revert(root: &str, hash: &str) -> Result<String, String> {
    run(root, &["revert", "--no-edit", hash])
}

/// Raw unified diff for ONE file — staged (index↔HEAD) or unstaged
/// (worktree↔index). The source for hunk-level staging.
pub fn file_diff(root: &str, file: &str, staged: bool) -> Result<String, String> {
    if staged {
        run(root, &["diff", "--cached", "--", file])
    } else {
        run(root, &["diff", "--", file])
    }
}

/// Apply a unified-diff patch to the index. `reverse` unstages a hunk (patch
/// taken from the staged diff); otherwise it stages the hunk (patch from the
/// unstaged diff).
pub fn apply_hunk(root: &str, patch: &str, reverse: bool) -> Result<String, String> {
    let mut args = vec!["apply", "--cached", "--whitespace=nowarn"];
    if reverse {
        args.push("--reverse");
    }
    args.push("-");
    let patch = if patch.ends_with('\n') {
        patch.to_string()
    } else {
        format!("{patch}\n")
    };
    run_stdin(root, &args, &patch).map(|_| "applied".to_string())
}

// ===========================================================================
// Visual commit graph (flagship — docs/16-git-experience.md)
//
// We compute a lane index per commit so the UI can render a GitKraken-style
// graph. Lane assignment is the standard "active lanes" sweep over commits in
// newest→oldest order; the frontend draws edges to each parent using a
// hash→(row,lane) map.
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphCommit {
    pub hash: String,
    pub short: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub date: String,
    pub subject: String,
    /// Decoration refs (branch/tag/HEAD names) from `%D`.
    pub refs: Vec<String>,
    /// Assigned horizontal lane (0 = leftmost).
    pub lane: usize,
    /// Lane color index (stable per lane).
    pub color: usize,
}

pub fn graph(root: &str, limit: u32) -> Result<Vec<GraphCommit>, String> {
    // %H hash, %P parents, %an author, %ae email, %ad date, %D refs, %s subject
    let fmt = "%H%x1f%P%x1f%an%x1f%ae%x1f%ad%x1f%D%x1f%s";
    let raw = run(
        root,
        &[
            "log",
            &format!("-{limit}"),
            "--date=short",
            "--branches",
            "--tags",
            "--remotes",
            "--topo-order",
            &format!("--pretty=format:{fmt}"),
        ],
    )?;

    let mut commits: Vec<GraphCommit> = raw
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split('\u{1f}').collect();
            if p.len() < 7 {
                return None;
            }
            let parents: Vec<String> = p[1]
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            let refs: Vec<String> = p[5]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Some(GraphCommit {
                hash: p[0].into(),
                short: p[0].chars().take(7).collect(),
                parents,
                author: p[2].into(),
                email: p[3].into(),
                date: p[4].into(),
                refs,
                subject: p[6].into(),
                lane: 0,
                color: 0,
            })
        })
        .collect();

    assign_lanes(&mut commits);
    Ok(commits)
}

/// Active-lanes sweep: each lane reserves the hash it expects next.
fn assign_lanes(commits: &mut [GraphCommit]) {
    // lanes[i] = Some(hash this lane is currently waiting to place)
    let mut lanes: Vec<Option<String>> = Vec::new();

    for c in commits.iter_mut() {
        // Find the leftmost lane reserved for this commit.
        let mut my_lane = lanes.iter().position(|l| l.as_deref() == Some(&c.hash));

        if my_lane.is_none() {
            // New tip — reuse a free lane or open a new one.
            my_lane = Some(match lanes.iter().position(|l| l.is_none()) {
                Some(i) => i,
                None => {
                    lanes.push(None);
                    lanes.len() - 1
                }
            });
        }
        let lane = my_lane.unwrap();
        c.lane = lane;
        c.color = lane % LANE_COLORS;

        // Free any *other* lanes that also waited on this commit (merges in).
        for (i, l) in lanes.iter_mut().enumerate() {
            if i != lane && l.as_deref() == Some(&c.hash) {
                *l = None;
            }
        }

        // This lane now expects the first parent; extra parents open new lanes.
        if let Some(first) = c.parents.first() {
            lanes[lane] = Some(first.clone());
            for p in c.parents.iter().skip(1) {
                match lanes.iter().position(|l| l.is_none()) {
                    Some(i) => lanes[i] = Some(p.clone()),
                    None => lanes.push(Some(p.clone())),
                }
            }
        } else {
            lanes[lane] = None;
        }
    }
}

const LANE_COLORS: usize = 8;

/// A heuristic commit-message suggestion derived from the staged changes.
/// This is the deterministic v1; the AI subsystem (docs/10) upgrades it to an
/// LLM-generated summary later. (docs/16 §AI-assisted)
pub fn suggest_commit_message(root: &str) -> Result<String, String> {
    let st = status(root)?;
    let staged: Vec<GitFileStatus> = st.files.iter().filter(|f| f.staged).cloned().collect();
    let has_staged = !staged.is_empty();
    let pool: Vec<GitFileStatus> = if has_staged { staged } else { st.files.clone() };
    if pool.is_empty() {
        return Ok(String::new());
    }

    // Inspect the actual staged (or unstaged) diff for a conventional-commit
    // style message: type(scope): summary. Deterministic v1 (LLM upgrade: docs/10).
    let diff = if has_staged {
        run(root, &["diff", "--cached"]).unwrap_or_default()
    } else {
        run(root, &["diff"]).unwrap_or_default()
    };

    // Symbols introduced by added lines (class/function/method names).
    let mut added_syms: Vec<String> = Vec::new();
    for line in diff.lines() {
        if !line.starts_with('+') || line.starts_with("+++") {
            continue;
        }
        let l = line[1..].trim();
        for kw in ["class ", "interface ", "trait ", "enum ", "function "] {
            if let Some(p) = l.find(kw) {
                let after = l[p + kw.len()..].trim_start();
                let name: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() && !added_syms.contains(&name) {
                    added_syms.push(name);
                }
                break;
            }
        }
    }

    let any = |pred: &dyn Fn(&GitFileStatus) -> bool| pool.iter().any(|f| pred(f));
    let all = |pred: &dyn Fn(&GitFileStatus) -> bool| pool.iter().all(|f| pred(f));

    // Conventional type from paths + change kinds.
    let ty = if all(&|f| f.path.ends_with(".md")) {
        "docs"
    } else if all(&|f| f.path.contains("tests/") || f.path.contains("Test.php")) {
        "test"
    } else if all(&|f| f.path.contains("config/") || f.path.starts_with(".env") || f.path.ends_with(".json")) {
        "chore"
    } else if any(&|f| matches!(f.label.as_str(), "added" | "untracked")) && !added_syms.is_empty() {
        "feat"
    } else if !added_syms.is_empty() {
        "feat"
    } else {
        "refactor"
    };

    // Scope = most common immediate parent directory name.
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in &pool {
        let parts: Vec<&str> = f.path.split('/').collect();
        if parts.len() >= 2 {
            *counts.entry(parts[parts.len() - 2].to_lowercase()).or_insert(0) += 1;
        }
    }
    let scope = counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(s, _)| s)
        .filter(|s| !s.is_empty() && s != "app");

    let join = |v: &[String]| {
        if v.len() <= 3 {
            v.join(", ")
        } else {
            format!("{} +{} more", v[..3].join(", "), v.len() - 3)
        }
    };

    let summary = if !added_syms.is_empty() {
        format!("add {}", join(&added_syms))
    } else if pool.len() == 1 {
        let base = pool[0].path.rsplit('/').next().unwrap_or("").to_string();
        match pool[0].label.as_str() {
            "deleted" => format!("remove {}", base),
            _ => format!("update {}", base),
        }
    } else {
        format!("update {} files", pool.len())
    };

    Ok(match scope {
        Some(s) => format!("{ty}({s}): {summary}"),
        None => format!("{ty}: {summary}"),
    })
}
