//! Context management tools — allow the model to inspect and optimize its context window.
//!
//! These tools are "virtual" — they don't access the filesystem directly but
//! interact with the agent's `ContextMap` to manage what's loaded and at what detail level.
//! They are dispatched specially in `tool_dispatch.rs` since they need mutable agent state.

use serde_json::{json, Value};

/// Tool name constants for context management tools.
pub const CONTEXT_STATUS: &str = "context_status";
pub const CONTEXT_FOCUS: &str = "context_focus";
pub const CONTEXT_EVICT: &str = "context_evict";
pub const CONTEXT_RECOMMEND: &str = "context_recommend";
pub const CONTEXT_LOAD_SKELETON: &str = "context_load_skeleton";
pub const CONTEXT_BULK_READ: &str = "context_bulk_read";
pub const CONTEXT_SUMMARY: &str = "context_summary";

/// All context tool names (for checking in dispatch).
pub const CONTEXT_TOOL_NAMES: &[&str] = &[
    CONTEXT_STATUS,
    CONTEXT_FOCUS,
    CONTEXT_EVICT,
    CONTEXT_RECOMMEND,
    CONTEXT_LOAD_SKELETON,
    CONTEXT_BULK_READ,
    CONTEXT_SUMMARY,
];

/// Check if a tool name is a context management tool.
pub fn is_context_tool(name: &str) -> bool {
    CONTEXT_TOOL_NAMES.contains(&name)
}

/// Generate tool descriptions for the context tools (used in system prompt).
pub fn context_tool_descriptions() -> Vec<ToolDesc> {
    vec![
        ToolDesc {
            name: CONTEXT_STATUS,
            description: "Show current context window status: budget usage, loaded files at each level (L1 tree / L2 skeleton / L3 full), and recommendations.",
            schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDesc {
            name: CONTEXT_FOCUS,
            description: "Promote files relevant to a query to full detail (L3). Automatically downgrades less relevant files to make room. Use this instead of reading many files manually.",
            schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to find relevant files (e.g., 'context compression', 'token estimation', 'file_read tool')"
                    },
                    "max_files": {
                        "type": "integer",
                        "description": "Maximum number of files to promote to L3 (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDesc {
            name: CONTEXT_EVICT,
            description: "Remove a file from context (downgrade to L1 tree level), freeing tokens for other content.",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to evict from context"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDesc {
            name: CONTEXT_RECOMMEND,
            description: "Get AI-powered recommendations for what context to load or evict for the current task. Returns a list of files to promote and files to evict with reasons.",
            schema: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Description of what you're trying to do (used to determine relevance)"
                    }
                },
                "required": ["task"]
            }),
        },
        ToolDesc {
            name: CONTEXT_LOAD_SKELETON,
            description: "Load a file at skeleton level (L2) — shows function/struct/trait signatures without bodies. Much cheaper than full read (~90% fewer tokens).",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to load at skeleton level"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDesc {
            name: CONTEXT_BULK_READ,
            description: "Read multiple source files in parallel into the context. Much faster than reading one at a time. Automatically manages budget by compressing older files. Use glob pattern to select files.",
            schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern for files to read (e.g., 'src/agent/*.rs', 'src/**/*.rs')"
                    },
                    "max_files": {
                        "type": "integer",
                        "description": "Maximum number of files to read (default: 20)",
                        "default": 20
                    }
                },
                "required": ["pattern"]
            }),
        },
        ToolDesc {
            name: CONTEXT_SUMMARY,
            description: "Generate a structured per-module summary of the codebase. Shows file counts, function/struct counts per module, and what's loaded at each level. Use this to understand the codebase without reading every file.",
            schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    ]
}

/// Lightweight descriptor for context tools (not the full Tool trait).
pub struct ToolDesc {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}
