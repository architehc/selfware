use super::*;
use crate::evolve::diagnostics::{
    report_from_cargo_output, AnalysisKind, AnalysisReport, CompilerDiagnostic, DiagnosticSpan,
};
use tempfile::tempdir;

fn report_with(diagnostics: Vec<CompilerDiagnostic>) -> AnalysisReport {
    AnalysisReport {
        kind: AnalysisKind::Check,
        label: "Cargo check".to_string(),
        command: vec!["cargo".into(), "check".into()],
        success: false,
        exit_code: Some(101),
        duration_ms: 150,
        errors: diagnostics.iter().filter(|d| d.level == "error").count(),
        diagnostics,
        warnings: 0,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        evidence_complete: true,
    }
}

fn suggestion(
    file: &str,
    bytes: (usize, usize),
    text: &str,
    applicability: &str,
) -> DiagnosticSpan {
    DiagnosticSpan {
        file: file.to_string(),
        line_start: 1,
        line_end: 1,
        column_start: 1,
        column_end: 1,
        is_primary: true,
        byte_start: Some(bytes.0),
        byte_end: Some(bytes.1),
        suggested_replacement: Some(text.to_string()),
        suggestion_applicability: Some(applicability.to_string()),
        ..Default::default()
    }
}

fn error_with_help(help_spans: Vec<Vec<DiagnosticSpan>>) -> CompilerDiagnostic {
    CompilerDiagnostic {
        level: "error".to_string(),
        code: Some("E0308".to_string()),
        message: "mismatched types".to_string(),
        children: help_spans
            .into_iter()
            .map(|spans| CompilerDiagnostic {
                level: "help".to_string(),
                message: "consider borrowing here".to_string(),
                spans,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

/// A real rustc 1.9x `--error-format=json` message (explanation trimmed),
/// as cargo wraps it: line 3 holds a two-byte `é`, so the byte offset of the
/// suggestion (102) differs from what the column (18) would give if it were
/// used as a byte index.
const RUSTC_E0308: &str = r#"{"reason":"compiler-message","message":{"$message_type":"diagnostic","message":"mismatched types","code":{"code":"E0308","explanation":"..."},"level":"error","spans":[{"file_name":"src/main.rs","byte_start":102,"byte_end":103,"line_start":4,"line_end":4,"column_start":18,"column_end":19,"is_primary":true,"text":[],"label":"expected `&String`, found `String`","suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[{"message":"function defined here","code":null,"level":"note","spans":[{"file_name":"src/main.rs","byte_start":3,"byte_end":7,"line_start":1,"line_end":1,"column_start":4,"column_end":8,"is_primary":true,"text":[],"label":null,"suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[],"rendered":null},{"message":"consider borrowing here","code":null,"level":"help","spans":[{"file_name":"src/main.rs","byte_start":102,"byte_end":102,"line_start":4,"line_end":4,"column_start":18,"column_end":18,"is_primary":true,"text":[],"label":null,"suggested_replacement":"&","suggestion_applicability":"MachineApplicable","expansion":null}],"children":[],"rendered":null}],"rendered":"error[E0308]: mismatched types\n"}}"#;

const E0308_SOURCE: &str = "fn take(v: &String) -> usize { v.len() }\nfn main() {\n    let s = String::from(\"é\");\n    let n = take(s);\n    let unused = 1;\n    println!(\"{n}\");\n}\n";

fn project_with_main(source: &str) -> tempfile::TempDir {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/main.rs"), source).unwrap();
    tmp
}

fn real_report(copies: usize) -> AnalysisReport {
    let stdout = vec![RUSTC_E0308; copies].join("\n");
    report_from_cargo_output(
        AnalysisKind::Check,
        vec!["cargo".into(), "check".into()],
        false,
        Some(101),
        10,
        stdout.as_bytes(),
        b"",
    )
}

#[test]
fn test_extract_machine_fixes_from_diagnostics_and_children() {
    let report = real_report(1);
    assert_eq!(report.errors, 1);
    let fixes = DiagnosticRepairEngine::new().extract_machine_fixes(&report);
    assert_eq!(fixes.len(), 1, "only the help child carries a suggestion");
    assert_eq!(fixes[0].file, "src/main.rs");
    assert_eq!(fixes[0].replacement, "&");
    assert_eq!(
        (fixes[0].byte_start, fixes[0].byte_end),
        (Some(102), Some(102))
    );
    assert_eq!(fixes[0].line_start, 4);
}

#[test]
fn only_machine_applicable_suggestions_are_extracted() {
    let report = report_with(vec![error_with_help(vec![
        vec![suggestion("src/main.rs", (0, 1), "a", "MaybeIncorrect")],
        vec![suggestion("src/main.rs", (0, 1), "b", "HasPlaceholders")],
        vec![suggestion("src/main.rs", (0, 1), "c", "Unspecified")],
        vec![suggestion("src/main.rs", (0, 1), "d", "MachineApplicable")],
    ])]);
    let fixes = DiagnosticRepairEngine::new().extract_machine_fixes(&report);
    assert_eq!(fixes.len(), 1);
    assert_eq!(fixes[0].replacement, "d");
}

#[test]
fn real_rustc_suggestion_applies_at_byte_offsets_past_non_ascii_text() {
    let tmp = project_with_main(E0308_SOURCE);
    let engine = DiagnosticRepairEngine::new();
    let fixes = engine.extract_machine_fixes(&real_report(1));
    let applied = engine.apply_machine_fixes(tmp.path(), &fixes).unwrap();
    assert_eq!(applied.applied, 1);
    assert!(applied.skipped.is_empty());
    assert_eq!(applied.files_changed, vec!["src/main.rs".to_string()]);
    let repaired = fs::read_to_string(tmp.path().join("src/main.rs")).unwrap();
    assert!(repaired.contains("    let n = take(&s);\n"), "{repaired}");
    assert!(repaired.contains("String::from(\"é\")"));
}

#[test]
fn the_same_suggestion_reported_twice_is_applied_once() {
    // `cargo check --all-targets` compiles a crate as lib AND as test target
    // and reports the same error (and fix) for each.
    let tmp = project_with_main(E0308_SOURCE);
    let engine = DiagnosticRepairEngine::new();
    let fixes = engine.extract_machine_fixes(&real_report(2));
    assert_eq!(fixes.len(), 2);
    let applied = engine.apply_machine_fixes(tmp.path(), &fixes).unwrap();
    assert_eq!(applied.applied, 1);
    assert_eq!(applied.duplicates, 1);
    let repaired = fs::read_to_string(tmp.path().join("src/main.rs")).unwrap();
    assert!(
        repaired.contains("take(&s)") && !repaired.contains("&&s"),
        "{repaired}"
    );
}

#[test]
fn an_overlapping_alternative_suggestion_is_skipped_with_its_reason() {
    let tmp = project_with_main("let v = x;\n");
    let report = report_with(vec![error_with_help(vec![
        vec![suggestion("src/main.rs", (8, 9), "&x", "MachineApplicable")],
        vec![suggestion(
            "src/main.rs",
            (8, 9),
            "x.clone()",
            "MachineApplicable",
        )],
    ])]);
    let engine = DiagnosticRepairEngine::new();
    let applied = engine
        .apply_machine_fixes(tmp.path(), &engine.extract_machine_fixes(&report))
        .unwrap();
    assert_eq!(applied.applied, 1);
    assert_eq!(applied.skipped.len(), 1);
    assert!(applied.skipped[0].reason.contains("overlaps"));
    assert_eq!(
        fs::read_to_string(tmp.path().join("src/main.rs")).unwrap(),
        "let v = &x;\n"
    );
}

#[test]
fn a_multipart_suggestion_is_applied_whole() {
    let tmp = project_with_main("f(a, b);\n");
    let report = report_with(vec![error_with_help(vec![vec![
        suggestion("src/main.rs", (2, 3), "&a", "MachineApplicable"),
        suggestion("src/main.rs", (5, 6), "&b", "MachineApplicable"),
    ]])]);
    let engine = DiagnosticRepairEngine::new();
    let applied = engine
        .apply_machine_fixes(tmp.path(), &engine.extract_machine_fixes(&report))
        .unwrap();
    assert_eq!(applied.applied, 2);
    assert_eq!(
        fs::read_to_string(tmp.path().join("src/main.rs")).unwrap(),
        "f(&a, &b);\n"
    );
}

#[test]
fn fixes_outside_the_project_root_are_never_written() {
    let outside = tempdir().unwrap();
    let victim = outside.path().join("lib.rs");
    fs::write(&victim, "let v = x;\n").unwrap();
    let tmp = project_with_main("fn main() {}\n");
    let report = report_with(vec![error_with_help(vec![
        vec![suggestion(
            victim.to_str().unwrap(),
            (8, 9),
            "&x",
            "MachineApplicable",
        )],
        vec![suggestion("../lib.rs", (8, 9), "&x", "MachineApplicable")],
    ])]);
    let engine = DiagnosticRepairEngine::new();
    let applied = engine
        .apply_machine_fixes(tmp.path(), &engine.extract_machine_fixes(&report))
        .unwrap();
    assert_eq!(applied.applied, 0);
    assert_eq!(applied.skipped.len(), 2);
    assert!(applied
        .skipped
        .iter()
        .all(|s| s.reason.contains("not a project-relative path")));
    assert_eq!(fs::read_to_string(&victim).unwrap(), "let v = x;\n");
}

#[test]
fn a_range_splitting_a_character_is_skipped() {
    let tmp = project_with_main("let s = \"é\";\n");
    // byte 10 is inside the two-byte `é` (bytes 9..11).
    let report = report_with(vec![error_with_help(vec![vec![suggestion(
        "src/main.rs",
        (10, 10),
        "x",
        "MachineApplicable",
    )]])]);
    let engine = DiagnosticRepairEngine::new();
    let applied = engine
        .apply_machine_fixes(tmp.path(), &engine.extract_machine_fixes(&report))
        .unwrap();
    assert_eq!(applied.applied, 0);
    assert!(applied.skipped[0].reason.contains("splits a character"));
}

#[test]
fn test_apply_machine_fixes_modifies_file_correctly() {
    // No byte offsets: the line/column pair (characters) is resolved.
    let tmp = project_with_main("fn main() {\n    let val = 42;\n    take_ownership(val);\n}\n");
    let fix = MachineApplicableFix {
        file: "src/main.rs".to_string(),
        line_start: 3,
        line_end: 3,
        column_start: 20,
        column_end: 23,
        byte_start: None,
        byte_end: None,
        replacement: "&val".to_string(),
        diagnostic_message: "consider borrowing".to_string(),
        suggestion_id: 0,
    };
    let applied = DiagnosticRepairEngine::new()
        .apply_machine_fixes(tmp.path(), &[fix])
        .unwrap();
    assert_eq!(applied.applied, 1);
    let modified = fs::read_to_string(tmp.path().join("src/main.rs")).unwrap();
    assert!(modified.contains("take_ownership(&val);"));
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(f)
}

#[test]
fn repair_loop_is_bounded_by_its_fuel() {
    // A check that always fails and always offers a fix (an insertion at
    // offset 0): the loop must stop after exactly `max_rounds` passes
    // (formal/EvolutionBounds.lean E1, repair_loop_bounded).
    let tmp = project_with_main("x\n");
    let engine = DiagnosticRepairEngine::new();
    let max_rounds = 3;
    let outcome = block_on(engine.run_repair_loop(tmp.path(), max_rounds, || async {
        Ok(report_with(vec![error_with_help(vec![vec![suggestion(
            "src/main.rs",
            (0, 0),
            "y",
            "MachineApplicable",
        )]])]))
    }))
    .unwrap();
    assert_eq!(outcome.stop, RepairStop::FuelExhausted);
    assert_eq!(outcome.rounds, max_rounds);
    assert_eq!(outcome.checks_run, max_rounds + 1);
    assert_eq!(outcome.fixes_applied, max_rounds);
    assert_eq!(
        fs::read_to_string(tmp.path().join("src/main.rs")).unwrap(),
        "yyyx\n"
    );
}

#[test]
fn repair_loop_stops_when_the_code_compiles_or_no_fix_exists() {
    let tmp = project_with_main(E0308_SOURCE);
    let engine = DiagnosticRepairEngine::new();
    let calls = std::cell::Cell::new(0usize);
    let outcome = block_on(engine.run_repair_loop(tmp.path(), 5, || {
        calls.set(calls.get() + 1);
        let n = calls.get();
        async move {
            let mut r = real_report(1);
            r.success = n > 1;
            Ok(r)
        }
    }))
    .unwrap();
    assert_eq!(outcome.stop, RepairStop::Compiles);
    assert_eq!(
        (outcome.rounds, outcome.fixes_applied, outcome.checks_run),
        (1, 1, 2)
    );

    let tmp = project_with_main("fn main() {}\n");
    let outcome = block_on(engine.run_repair_loop(tmp.path(), 5, || async {
        Ok(report_with(vec![CompilerDiagnostic {
            level: "error".into(),
            message: "no suggestion".into(),
            ..Default::default()
        }]))
    }))
    .unwrap();
    assert_eq!(outcome.stop, RepairStop::NoApplicableFix);
    assert_eq!(outcome.rounds, 0);
}

#[test]
fn test_synthesize_repair_guidance_for_borrow_and_type_errors() {
    let diag = CompilerDiagnostic {
        level: "error".to_string(),
        code: Some("E0382".to_string()),
        message: "use of moved value: `buffer`".to_string(),
        rendered: Some("error[E0382]: use of moved value".into()),
        spans: vec![DiagnosticSpan {
            file: "src/buffer.rs".to_string(),
            line_start: 45,
            line_end: 45,
            column_start: 9,
            column_end: 15,
            is_primary: true,
            label: Some("value moved here".into()),
            ..Default::default()
        }],
        children: Vec::new(),
    };

    let guidance =
        DiagnosticRepairEngine::new().synthesize_repair_guidance(&report_with(vec![diag]));

    assert_eq!(guidance.len(), 1);
    assert_eq!(guidance[0].file, "src/buffer.rs");
    assert_eq!(guidance[0].line, 45);
    assert_eq!(guidance[0].code.as_deref(), Some("E0382"));
    assert!(guidance[0].suggested_action.contains(".clone()"));
}
