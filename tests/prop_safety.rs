use proptest::prelude::*;
use selfware::api::types::{ToolCall, ToolFunction};
use selfware::config::SafetyConfig;
use selfware::safety::SafetyChecker;

fn shell_call(command: &str) -> ToolCall {
    ToolCall {
        id: "prop-test".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "shell_exec".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        },
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_shell_checker_never_panics(command in ".*") {
        let checker = SafetyChecker::new(&SafetyConfig::default());
        let call = shell_call(&command);
        let _ = checker.check_tool_call(&call);
    }
}
