use super::resolve_step_token_counts;

#[test]
fn uses_reported_values_when_present() {
    let (input, output) = resolve_step_token_counts(Some(1200), Some(300), 999, 999);
    assert_eq!(input, 1200);
    assert_eq!(output, 300);
}

#[test]
fn falls_back_to_estimates_when_usage_absent() {
    // Backend omitted usage entirely (common on local vLLM/SGLang).
    let (input, output) = resolve_step_token_counts(None, None, 1500, 220);
    assert_eq!(input, 1500);
    assert_eq!(output, 220);
}

#[test]
fn mixes_reported_and_estimated_per_field() {
    // Provider gave prompt tokens but not completion tokens.
    let (input, output) = resolve_step_token_counts(Some(1200), None, 999, 220);
    assert_eq!(input, 1200);
    assert_eq!(output, 220);
}
