import re

with open('src/api/mod.rs', 'r') as f:
    content = f.read()

pattern1 = r"""        let mut messages = messages;
        match thinking \{
            ThinkingMode::Enabled => \{\}
            ThinkingMode::Disabled => \{
                // For Qwen3 and similar models, explicitly instruct it not to use thinking blocks
                let sys_msg = crate::api::types::Message::system\("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately."\);
                messages.insert\(0, sys_msg\);
                // Also append stop token if possible, though not directly standardized in payload here without overwriting others
            \}
            ThinkingMode::Budget\(tokens\) => \{
                body\["thinking"\] = serde_json::json!\(\{
                    "type": "enabled",
                    "budget_tokens": tokens
                \}\);
            \}
        \}
        
        let mut body = serde_json::json!\(\{"""

replacement1 = """        let mut messages = messages;
        if let ThinkingMode::Disabled = thinking {
            let sys_msg = crate::api::types::Message::system("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.");
            messages.insert(0, sys_msg);
        }
        
        let mut body = serde_json::json!({"""

pattern2 = r"""        let mut messages = messages;
        match thinking \{
            ThinkingMode::Enabled => \{\}
            ThinkingMode::Disabled => \{
                let sys_msg = crate::api::types::Message::system\("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately."\);
                messages.insert\(0, sys_msg\);
            \}
            ThinkingMode::Budget\(tokens\) => \{
                body\["thinking"\] = serde_json::json!\(\{
                    "type": "enabled",
                    "budget_tokens": tokens
                \}\);
            \}
        \}

        let mut body = serde_json::json!\(\{"""

replacement2 = """        let mut messages = messages;
        if let ThinkingMode::Disabled = thinking {
            let sys_msg = crate::api::types::Message::system("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.");
            messages.insert(0, sys_msg);
        }

        let mut body = serde_json::json!({"""

content = re.sub(pattern1, replacement1, content)
content = re.sub(pattern2, replacement2, content)

# Now we need to handle the Budget mode properly by appending to body
budget_add1 = """        if let Some(ref tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }
"""
budget_add2 = """        if let Some(ref tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }
        
        if let ThinkingMode::Budget(tokens) = thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": tokens
            });
        }
"""
content = content.replace(budget_add1, budget_add2)

with open('src/api/mod.rs', 'w') as f:
    f.write(content)
