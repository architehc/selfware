use super::*;

// ── VisualScore ──────────────────────────────────────────────────

#[test]
fn test_visual_score_compute_overall() {
    let mut score = VisualScore {
        composition: 80.0,
        hierarchy: 70.0,
        readability: 90.0,
        consistency: 85.0,
        accessibility: 75.0,
        overall: 0.0,
        suggestions: vec![],
    };
    score.compute_overall();
    // 80*0.20 + 70*0.20 + 90*0.25 + 85*0.15 + 75*0.20 = 16+14+22.5+12.75+15 = 80.25
    assert!((score.overall - 80.25).abs() < 0.01);
}

#[test]
fn test_visual_score_compute_overall_all_zeros() {
    let mut score = VisualScore::default();
    score.compute_overall();
    assert!((score.overall - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_visual_score_compute_overall_all_100() {
    let mut score = VisualScore {
        composition: 100.0,
        hierarchy: 100.0,
        readability: 100.0,
        consistency: 100.0,
        accessibility: 100.0,
        overall: 0.0,
        suggestions: vec![],
    };
    score.compute_overall();
    assert!((score.overall - 100.0).abs() < 0.01);
}

#[test]
fn test_visual_score_compute_overall_weights_sum_to_one() {
    // Verify weights: 0.20 + 0.20 + 0.25 + 0.15 + 0.20 = 1.0
    let mut score = VisualScore {
        composition: 50.0,
        hierarchy: 50.0,
        readability: 50.0,
        consistency: 50.0,
        accessibility: 50.0,
        overall: 0.0,
        suggestions: vec![],
    };
    score.compute_overall();
    assert!((score.overall - 50.0).abs() < 0.01);
}

#[test]
fn test_visual_score_default() {
    let score = VisualScore::default();
    assert_eq!(score.composition, 0.0);
    assert_eq!(score.hierarchy, 0.0);
    assert_eq!(score.readability, 0.0);
    assert_eq!(score.consistency, 0.0);
    assert_eq!(score.accessibility, 0.0);
    assert_eq!(score.overall, 0.0);
    assert!(score.suggestions.is_empty());
}

#[test]
fn test_visual_score_serde_roundtrip() {
    let score = VisualScore {
        composition: 85.0,
        hierarchy: 70.0,
        readability: 90.0,
        consistency: 80.0,
        accessibility: 75.0,
        overall: 80.0,
        suggestions: vec!["Increase contrast".into(), "Align grid".into()],
    };
    let json = serde_json::to_string(&score).unwrap();
    let parsed: VisualScore = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.composition, 85.0);
    assert_eq!(parsed.suggestions.len(), 2);
    assert_eq!(parsed.suggestions[0], "Increase contrast");
}

#[test]
fn test_visual_score_serde_suggestions_default() {
    // suggestions should default to empty when missing
    let json = r#"{"composition":80,"hierarchy":70,"readability":90,"consistency":85,"accessibility":75,"overall":80}"#;
    let score: VisualScore = serde_json::from_str(json).unwrap();
    assert!(score.suggestions.is_empty());
}

// ── parse_critic_response ────────────────────────────────────────

#[test]
fn test_parse_critic_response_clean_json() {
    let json = r#"{"composition":85,"hierarchy":70,"readability":90,"consistency":80,"accessibility":75,"overall":80,"suggestions":["Increase contrast"]}"#;
    let score = parse_critic_response(json).unwrap();
    assert_eq!(score.composition, 85.0);
    assert_eq!(score.hierarchy, 70.0);
    assert_eq!(score.readability, 90.0);
    assert_eq!(score.consistency, 80.0);
    assert_eq!(score.accessibility, 75.0);
    assert_eq!(score.overall, 80.0);
    assert_eq!(score.suggestions.len(), 1);
}

#[test]
fn test_parse_critic_response_with_markdown_fences() {
    let response = "Here is my analysis:\n```json\n{\"composition\":90,\"hierarchy\":85,\"readability\":88,\"consistency\":92,\"accessibility\":80,\"overall\":87,\"suggestions\":[\"Add focus indicators\"]}\n```\nDone.";
    let score = parse_critic_response(response).unwrap();
    assert_eq!(score.composition, 90.0);
    assert_eq!(score.overall, 87.0);
}

#[test]
fn test_parse_critic_response_with_leading_text() {
    let response = "The design looks good overall. {\"composition\":70,\"hierarchy\":60,\"readability\":80,\"consistency\":75,\"accessibility\":65,\"overall\":70,\"suggestions\":[]}";
    let score = parse_critic_response(response).unwrap();
    assert_eq!(score.composition, 70.0);
}

#[test]
fn test_parse_critic_response_with_trailing_text() {
    let response = "{\"composition\":70,\"hierarchy\":60,\"readability\":80,\"consistency\":75,\"accessibility\":65,\"overall\":70,\"suggestions\":[]} That's my assessment.";
    let score = parse_critic_response(response).unwrap();
    assert_eq!(score.overall, 70.0);
}

#[test]
fn test_parse_critic_response_empty_suggestions() {
    let json = r#"{"composition":50,"hierarchy":50,"readability":50,"consistency":50,"accessibility":50,"overall":50,"suggestions":[]}"#;
    let score = parse_critic_response(json).unwrap();
    assert!(score.suggestions.is_empty());
}

#[test]
fn test_parse_critic_response_multiple_suggestions() {
    let json = r#"{"composition":50,"hierarchy":50,"readability":50,"consistency":50,"accessibility":50,"overall":50,"suggestions":["Fix A","Fix B","Fix C"]}"#;
    let score = parse_critic_response(json).unwrap();
    assert_eq!(score.suggestions.len(), 3);
}

#[test]
fn test_parse_critic_response_invalid_json() {
    let result = parse_critic_response("This is not JSON at all");
    assert!(result.is_err());
}

#[test]
fn test_parse_critic_response_empty_string() {
    let result = parse_critic_response("");
    assert!(result.is_err());
}

#[test]
fn test_parse_critic_response_partial_json() {
    let result = parse_critic_response("{\"composition\": 80");
    assert!(result.is_err());
}

#[test]
fn test_parse_critic_response_with_float_scores() {
    let json = r#"{"composition":85.5,"hierarchy":70.3,"readability":90.1,"consistency":80.7,"accessibility":75.9,"overall":80.5,"suggestions":[]}"#;
    let score = parse_critic_response(json).unwrap();
    assert!((score.composition - 85.5).abs() < 0.01);
    assert!((score.hierarchy - 70.3).abs() < 0.01);
}

// ── build_critic_prompt ──────────────────────────────────────────

#[test]
fn test_build_critic_prompt_first_iteration() {
    let prompt = build_critic_prompt("Build a landing page", None, 0);
    assert!(prompt.contains("landing page"));
    assert!(prompt.contains("iteration 1"));
    assert!(!prompt.contains("Previous iteration"));
    assert!(prompt.contains("JSON"));
    assert!(prompt.contains("composition"));
}

#[test]
fn test_build_critic_prompt_with_previous() {
    let prev = VisualScore {
        composition: 60.0,
        hierarchy: 50.0,
        readability: 70.0,
        consistency: 55.0,
        accessibility: 65.0,
        overall: 60.0,
        suggestions: vec!["Fix alignment".into()],
    };
    let prompt = build_critic_prompt("Build a dashboard", Some(&prev), 1);
    assert!(prompt.contains("iteration 2"));
    assert!(prompt.contains("Previous iteration scores"));
    assert!(prompt.contains("Fix alignment"));
    assert!(prompt.contains("Composition: 60"));
    assert!(prompt.contains("Hierarchy: 50"));
}

#[test]
fn test_build_critic_prompt_multiple_suggestions() {
    let prev = VisualScore {
        suggestions: vec!["Fix A".into(), "Fix B".into()],
        ..Default::default()
    };
    let prompt = build_critic_prompt("Task", Some(&prev), 2);
    assert!(prompt.contains("iteration 3"));
    assert!(prompt.contains("Fix A; Fix B"));
}

#[test]
fn test_build_critic_prompt_empty_suggestions() {
    let prev = VisualScore {
        suggestions: vec![],
        ..Default::default()
    };
    let prompt = build_critic_prompt("Task", Some(&prev), 0);
    assert!(prompt.contains("Previous suggestions:"));
}

#[test]
fn test_build_critic_prompt_iteration_numbering() {
    for i in 0..5 {
        let prompt = build_critic_prompt("Task", None, i);
        assert!(prompt.contains(&format!("iteration {}", i + 1)));
    }
}

// ── VisualFeedbackLoop ───────────────────────────────────────────

#[test]
fn test_visual_feedback_loop_default() {
    let vfl = VisualFeedbackLoop::default();
    assert_eq!(vfl.max_iterations, 5);
    assert!((vfl.quality_threshold - 0.8).abs() < f64::EPSILON);
    assert_eq!(vfl.vision_model_id, "vision");
    assert!(matches!(vfl.capture_method, CaptureMethod::Screen));
}

#[test]
fn test_visual_feedback_loop_custom() {
    let vfl = VisualFeedbackLoop {
        max_iterations: 10,
        quality_threshold: 0.95,
        vision_model_id: "qwen3.5-27b".to_string(),
        capture_method: CaptureMethod::BrowserUrl("http://localhost:3000".to_string()),
    };
    assert_eq!(vfl.max_iterations, 10);
    assert!((vfl.quality_threshold - 0.95).abs() < f64::EPSILON);
}

// ── CaptureMethod ────────────────────────────────────────────────

#[test]
fn test_capture_method_serde_roundtrip() {
    let methods = vec![
        CaptureMethod::Screen,
        CaptureMethod::Window("Firefox".into()),
        CaptureMethod::BrowserUrl("http://localhost:3000".into()),
    ];
    for method in methods {
        let json = serde_json::to_string(&method).unwrap();
        let parsed: CaptureMethod = serde_json::from_str(&json).unwrap();
        let _ = format!("{:?}", parsed);
    }
}

#[test]
fn test_capture_method_debug() {
    assert!(format!("{:?}", CaptureMethod::Screen).contains("Screen"));
    assert!(format!("{:?}", CaptureMethod::Window("vim".into())).contains("vim"));
    assert!(format!("{:?}", CaptureMethod::BrowserUrl("http://x".into())).contains("http://x"));
}

// ── VisualLoopResult ─────────────────────────────────────────────

#[test]
fn test_visual_loop_result_serde() {
    let result = VisualLoopResult {
        iterations: 3,
        threshold_met: true,
        score_history: vec![VisualScore::default()],
        final_score: VisualScore {
            overall: 85.0,
            ..Default::default()
        },
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: VisualLoopResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.iterations, 3);
    assert!(parsed.threshold_met);
    assert_eq!(parsed.score_history.len(), 1);
    assert!((parsed.final_score.overall - 85.0).abs() < f64::EPSILON);
}

#[test]
fn test_visual_loop_result_not_met() {
    let result = VisualLoopResult {
        iterations: 5,
        threshold_met: false,
        score_history: vec![
            VisualScore {
                overall: 30.0,
                ..Default::default()
            },
            VisualScore {
                overall: 45.0,
                ..Default::default()
            },
            VisualScore {
                overall: 55.0,
                ..Default::default()
            },
            VisualScore {
                overall: 60.0,
                ..Default::default()
            },
            VisualScore {
                overall: 65.0,
                ..Default::default()
            },
        ],
        final_score: VisualScore {
            overall: 65.0,
            ..Default::default()
        },
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: VisualLoopResult = serde_json::from_str(&json).unwrap();
    assert!(!parsed.threshold_met);
    assert_eq!(parsed.score_history.len(), 5);
}

#[test]
fn test_visual_loop_result_empty_history() {
    let result = VisualLoopResult {
        iterations: 0,
        threshold_met: false,
        score_history: vec![],
        final_score: VisualScore::default(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: VisualLoopResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.iterations, 0);
    assert!(parsed.score_history.is_empty());
}
