use super::*;

struct MockToolPrompt;

impl ToolPrompt for MockToolPrompt {
    fn description(&self) -> String {
        "A mock tool for testing.".to_string()
    }

    fn when_to_use(&self) -> String {
        "Use this when testing the ToolPrompt trait.".to_string()
    }

    fn examples(&self) -> Vec<ToolExample> {
        vec![ToolExample {
            description: "Basic usage".to_string(),
            input: serde_json::json!({"arg": "value"}),
            output: Some("Result".to_string()),
        }]
    }

    fn important_notes(&self) -> Option<String> {
        Some("This is a test tool.".to_string())
    }
}

#[test]
fn test_tool_prompt_methods() {
    let tool = MockToolPrompt;

    assert_eq!(tool.description(), "A mock tool for testing.");
    assert_eq!(
        tool.when_to_use(),
        "Use this when testing the ToolPrompt trait."
    );
    assert_eq!(tool.examples().len(), 1);
    assert_eq!(
        tool.important_notes(),
        Some("This is a test tool.".to_string())
    );
}

#[test]
fn test_tool_example_structure() {
    let example = ToolExample {
        description: "Test example".to_string(),
        input: serde_json::json!({"key": "value"}),
        output: Some("Expected output".to_string()),
    };

    assert_eq!(example.description, "Test example");
    assert_eq!(example.input["key"], "value");
    assert_eq!(example.output, Some("Expected output".to_string()));
}
