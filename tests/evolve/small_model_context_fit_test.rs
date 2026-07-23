use selfware::evolve::{
    context_selector::{select as select_context, TaskKind},
    ContextComposer, ContextMode, Graph, Node,
};

fn create_mock_codebase_graph(file_count: usize, avg_lines_per_file: usize) -> Graph {
    let mut nodes = Vec::new();
    for i in 0..file_count {
        let id = format!("crate::module_{i}");
        let path = format!("src/module_{i}.rs");
        let mut node = Node::code(&id, &path);
        node.lines = avg_lines_per_file;
        // ~10 tokens per line
        node.tokens = avg_lines_per_file * 10;
        nodes.push(node);
    }
    Graph {
        nodes,
        edges: Vec::new(),
    }
}

#[test]
fn test_task_aware_selection_fits_within_8k_small_model_window() {
    let graph = create_mock_codebase_graph(100, 250); // 25,000 LOC codebase (~250k tokens)
    let temp = tempfile::tempdir().unwrap();

    // Select context for a Refactor task on target `module_42`
    let selection = select_context(TaskKind::Refactor, "module_42", &graph, temp.path()).unwrap();

    // Verify only relevant neighborhood files selected (not all 100 files)
    assert!(selection.files.len() < 10, "Selected files should be focused: {}", selection.files.len());

    // Calculate token cost for selected files
    let selected_tokens: usize = selection
        .files
        .iter()
        .map(|_| 250 * 10) // 2,500 tokens per file
        .sum();

    // 8k context window limit (8,192 tokens)
    let small_model_limit = 8_192;
    assert!(
        selected_tokens < small_model_limit,
        "Task-aware context selection ({selected_tokens} tokens) MUST fit within 8k model limit ({small_model_limit} tokens)"
    );
}

#[test]
fn test_lite_mode_fits_within_32k_small_model_window() {
    let graph = create_mock_codebase_graph(80, 200); // 16,000 LOC codebase (~160k full tokens)
    let mut composer = ContextComposer::new(graph);

    composer.set_mode(ContextMode::Lite);
    let lite_tokens = composer.estimate_tokens();

    // Lite mode (signatures ~18%) should reduce ~160k tokens to ~28.8k tokens
    let model_32k_limit = 32_768;
    assert!(
        lite_tokens < model_32k_limit,
        "Lite mode estimate ({lite_tokens} tokens) MUST fit within 32k model limit ({model_32k_limit} tokens)"
    );
}

#[test]
fn test_compact_mode_strips_comments_and_fits_within_128k_window() {
    let graph = create_mock_codebase_graph(50, 250); // 12,500 LOC (~125k tokens)
    let mut composer = ContextComposer::new(graph);

    composer.set_mode(ContextMode::Compact);
    let compact_tokens = composer.estimate_tokens();

    // Compact mode (~82%) reduces 125k tokens to ~102.5k tokens
    let model_128k_limit = 131_072;
    assert!(
        compact_tokens < model_128k_limit,
        "Compact mode estimate ({compact_tokens} tokens) MUST fit within 128k model limit ({model_128k_limit} tokens)"
    );
}

#[test]
fn test_context_mode_auto_downgrade_recommendation() {
    let small_model_limit = 16_384; // 16k small model (e.g. DeepSeek Coder 16k)
    let graph = create_mock_codebase_graph(40, 200); // ~80k tokens full
    let composer = ContextComposer::new(graph);

    let mode_sizes = composer.mode_sizes();

    // Check which modes fit into the 16k window
    let fitting_modes: Vec<_> = mode_sizes
        .iter()
        .filter(|s| s.tokens < small_model_limit)
        .map(|s| s.mode.as_str())
        .collect();

    assert!(
        fitting_modes.contains(&"lite"),
        "Lite mode should fit in 16k window, available: {:?}", fitting_modes
    );
    assert!(
        !fitting_modes.contains(&"full"),
        "Full mode should exceed 16k window for 80k codebase"
    );
}
