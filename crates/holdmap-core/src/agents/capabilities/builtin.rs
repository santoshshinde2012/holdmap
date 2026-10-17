//! Documented built-in tools per product. These come from product documentation, not from
//! observation: an agent's version or policy may add, rename or disable some of them.

/// The documented built-in tools of a product, or an empty list when not catalogued.
pub fn builtin_tools(product: &str) -> &'static [&'static str] {
    match product {
        "claude-code" => &[
            "Bash",
            "Edit",
            "Glob",
            "Grep",
            "MultiEdit",
            "NotebookEdit",
            "Read",
            "Task",
            "TodoWrite",
            "WebFetch",
            "WebSearch",
            "Write",
        ],
        "codex" | "codex-desktop" => &[
            "apply_patch",
            "shell",
            "update_plan",
            "view_image",
            "web_search",
        ],
        "gemini" => &[
            "glob",
            "google_web_search",
            "list_directory",
            "read_file",
            "read_many_files",
            "replace",
            "run_shell_command",
            "save_memory",
            "search_file_content",
            "web_fetch",
            "write_file",
        ],
        _ => &[],
    }
}
