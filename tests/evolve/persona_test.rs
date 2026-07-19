use selfware::evolve::persona::ComponentPersona;
use selfware::evolve::Node;

#[test]
fn test_persona_generates_grounded_explanation() {
    let persona = ComponentPersona::new();
    let node = Node::code("agent", "src/agent");
    let explanation = persona.explain(&node).unwrap();
    assert!(explanation.contains("agent"));
    assert!(explanation.contains("src/agent"));
}
