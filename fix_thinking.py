import re

with open('src/api/mod.rs', 'r') as f:
    content = f.read()

def replace_thinking_mode(match):
    return """
        let mut messages = messages;
        match thinking {
            ThinkingMode::Enabled => {}
            ThinkingMode::Disabled => {
                // For Qwen3 and similar models, explicitly instruct it not to use thinking blocks
                let sys_msg = crate::api::types::Message::system("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.");
                messages.insert(0, sys_msg);
                // Also append stop token if possible, though not directly standardized in payload here without overwriting others
            }
            ThinkingMode::Budget(tokens) => {
                body["thinking"] = serde_json::json!({
                    "type": "enabled",
                    "budget_tokens": tokens
                });
            }
        }
        
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": false,
        });

        if let Some(ref tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }
"""

def replace_thinking_mode_stream(match):
    return """
        let mut messages = messages;
        match thinking {
            ThinkingMode::Enabled => {}
            ThinkingMode::Disabled => {
                let sys_msg = crate::api::types::Message::system("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.");
                messages.insert(0, sys_msg);
            }
            ThinkingMode::Budget(tokens) => {
                body["thinking"] = serde_json::json!({
                    "type": "enabled",
                    "budget_tokens": tokens
                });
            }
        }

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": true,
        });

        if let Some(ref tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }
"""

pattern1 = r"""        let mut body = serde_json::json!\(\{
            "model": self\.config\.model,
            "messages": messages,
            "temperature": self\.config\.temperature,
            "max_tokens": self\.config\.max_tokens,
            "stream": false,
        \}\);

        if let Some\(ref tools\) = tools \{
            body\["tools"\] = serde_json::json!\(tools\);
        \}

        match thinking \{
            ThinkingMode::Enabled => \{
                // Default behavior, no special config needed
            \}
            ThinkingMode::Disabled => \{
                body\["thinking"\] = serde_json::json!\(\{"type": "disabled"\}\);
            \}
            ThinkingMode::Budget\(tokens\) => \{
                body\["thinking"\] = serde_json::json!\(\{
                    "type": "enabled",
                    "budget_tokens": tokens
                \}\);
            \}
        \}"""

pattern2 = r"""        let mut body = serde_json::json!\(\{
            "model": self\.config\.model,
            "messages": messages,
            "temperature": self\.config\.temperature,
            "max_tokens": self\.config\.max_tokens,
            "stream": true,
        \}\);

        if let Some\(ref tools\) = tools \{
            body\["tools"\] = serde_json::json!\(tools\);
        \}

        match thinking \{
            ThinkingMode::Enabled => \{\}
            ThinkingMode::Disabled => \{
                body\["thinking"\] = serde_json::json!\(\{"type": "disabled"\}\);
            \}
            ThinkingMode::Budget\(tokens\) => \{
                body\["thinking"\] = serde_json::json!\(\{
                    "type": "enabled",
                    "budget_tokens": tokens
                \}\);
            \}
        \}"""

content = re.sub(pattern1, replace_thinking_mode(""), content)
content = re.sub(pattern2, replace_thinking_mode_stream(""), content)

with open('src/api/mod.rs', 'w') as f:
    f.write(content)

