//! Git metadata: repository root and current branch, read straight from `.git` (no `git` binary).

use std::fs;
use std::path::{Path, PathBuf};

/// The nearest directory at or above `dir` containing `.git` (dir or worktree file).
pub fn git_root(dir: &Path) -> Option<PathBuf> {
    let mut d = Some(dir);
    for _ in 0..12 {
        let cur = d?;
        if cur.join(".git").exists() {
            return Some(cur.to_path_buf());
        }
        d = cur.parent();
    }
    None
}

/// Current git branch for a project root (searches upwards for `.git`).
pub fn git_branch(root: &Path) -> Option<String> {
    let mut dir = Some(root);
    for _ in 0..6 {
        let d = dir?;
        let git = d.join(".git");
        let git_dir = if git.is_dir() {
            git
        } else if git.is_file() {
            // Worktrees / submodules: "gitdir: <path>"
            let content = fs::read_to_string(&git).ok()?;
            let p = PathBuf::from(content.trim().strip_prefix("gitdir:")?.trim());
            if p.is_absolute() {
                p
            } else {
                d.join(p)
            }
        } else {
            dir = d.parent();
            continue;
        };
        let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
        let head = head.trim();
        return Some(match head.strip_prefix("ref: refs/heads/") {
            Some(b) => b.to_string(),
            None => head.chars().take(7).collect(),
        });
    }
    None
}
