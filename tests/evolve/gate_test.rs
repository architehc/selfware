use selfware::evolve::gate::Gatekeeper;

#[test]
fn test_gatekeeper_passes_on_clean_repo() {
    let gate = Gatekeeper::new();
    let result = gate.check_code_gates().unwrap();
    assert!(result.passed);
}
