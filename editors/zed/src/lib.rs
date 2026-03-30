use zed_extension_api as zed;

/// Selfware Code Map — Zed extension scaffold.
///
/// Zed extensions compile to WASM and interact with the editor through
/// the `zed_extension_api` crate.  This file shows the intended structure;
/// fill in the bodies once the Zed extension API stabilises the panel /
/// command surface.

struct SelfwareCodeMap {
    /// Parsed contents of codegraph.json (kept in memory after activation).
    graph_json: Option<String>,
}

impl zed::Extension for SelfwareCodeMap {
    fn new() -> Self {
        Self { graph_json: None }
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        // Load codegraph.json from the workspace root on first call.
        if self.graph_json.is_none() {
            let root = worktree.root_path();
            let path = format!("{root}/codegraph.json");
            // In a real implementation: read the file via the Zed FS API
            // and deserialise into a typed graph structure.
            self.graph_json = Some(path);
        }

        // Return the rust-analyzer binary as the language server.
        // The codegraph data is used by our own commands, not by the LS.
        Ok(zed::Command {
            command: "rust-analyzer".into(),
            args: vec![],
            env: vec![],
        })
    }
}

// Register commands that will appear in the Zed command palette.
//
// Planned commands (implement when the Zed command API is available):
//   - selfware:open_codemap   — open a side panel with the graph
//   - selfware:context_add    — add the symbol under cursor to context
//   - selfware:context_remove — remove a symbol from context
//   - selfware:inspect        — show token cost / dependency info

zed::register_extension!(SelfwareCodeMap);
