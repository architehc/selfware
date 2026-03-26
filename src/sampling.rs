//! Dynamic sampling configuration for Qwen3.5-27B-FP8
//! Automatically selects optimal parameters based on task type

use serde::{Deserialize, Serialize};

/// Sampling mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMode {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub min_p: f32,
    pub presence_penalty: f32,
    pub repetition_penalty: f32,
    pub use_thinking: bool,
    pub description: String,
}

impl Default for SamplingMode {
    fn default() -> Self {
        // Default: Thinking precise (best for coding)
        Self {
            temperature: 0.6,
            top_p: 0.95,
            top_k: 20,
            min_p: 0.0,
            presence_penalty: 0.0,
            repetition_penalty: 1.0,
            use_thinking: true,
            description: "Precise coding mode".to_string(),
        }
    }
}

/// Predefined sampling modes for Qwen3.5-27B-FP8
pub struct SamplingModes;

impl SamplingModes {
    /// Thinking mode for general tasks (creative, exploratory)
    pub fn thinking_general() -> SamplingMode {
        SamplingMode {
            temperature: 1.0,
            top_p: 0.95,
            top_k: 20,
            min_p: 0.0,
            presence_penalty: 1.5,
            repetition_penalty: 1.0,
            use_thinking: true,
            description: "Thinking - General (creative)".to_string(),
        }
    }

    /// Thinking mode for precise coding tasks
    pub fn thinking_precise() -> SamplingMode {
        SamplingMode {
            temperature: 0.6,
            top_p: 0.95,
            top_k: 20,
            min_p: 0.0,
            presence_penalty: 0.0,
            repetition_penalty: 1.0,
            use_thinking: true,
            description: "Thinking - Precise coding".to_string(),
        }
    }

    /// Instruct mode for general tasks (factual, direct)
    pub fn instruct_general() -> SamplingMode {
        SamplingMode {
            temperature: 0.7,
            top_p: 0.8,
            top_k: 20,
            min_p: 0.0,
            presence_penalty: 1.5,
            repetition_penalty: 1.0,
            use_thinking: false,
            description: "Instruct - General".to_string(),
        }
    }

    /// Instruct mode for reasoning tasks (math, logic)
    pub fn instruct_reasoning() -> SamplingMode {
        SamplingMode {
            temperature: 1.0,
            top_p: 1.0,
            top_k: 40,
            min_p: 0.0,
            presence_penalty: 2.0,
            repetition_penalty: 1.0,
            use_thinking: false,
            description: "Instruct - Reasoning".to_string(),
        }
    }
}

/// Task classifier for selecting sampling mode
pub struct TaskClassifier;

impl TaskClassifier {
    /// Classify task and return appropriate sampling mode
    pub fn classify(prompt: &str) -> SamplingMode {
        let prompt_lower = prompt.to_lowercase();
        
        // Code-related tasks - use thinking precise
        let code_keywords = [
            "write", "implement", "create", "fix", "debug", "refactor",
            "function", "class", "method", "variable", "error", "bug",
            "syntax", "compile", "build", "test", "web", "html", "css",
            "javascript", "python", "rust", "go", "java", "typescript",
        ];
        
        // Reasoning tasks - use instruct reasoning
        let reasoning_keywords = [
            "analyze", "compare", "evaluate", "calculate", "solve",
            "math", "algorithm", "complexity", "optimize", "prove",
            "logic", "reason", "deduce", "infer",
        ];
        
        // Documentation tasks - use instruct general
        let doc_keywords = [
            "document", "explain", "describe", "summarize", "readme",
            "comment", "docstring", "guide", "tutorial", "example",
        ];
        
        // Creative/exploratory - use thinking general
        let creative_keywords = [
            "explore", "brainstorm", "design", "architecture", "pattern",
            "idea", "concept", "prototype", "experiment",
        ];
        
        // Count keyword matches
        let code_score = code_keywords.iter()
            .filter(|&&k| prompt_lower.contains(k))
            .count();
        let reasoning_score = reasoning_keywords.iter()
            .filter(|&&k| prompt_lower.contains(k))
            .count();
        let doc_score = doc_keywords.iter()
            .filter(|&&k| prompt_lower.contains(k))
            .count();
        let creative_score = creative_keywords.iter()
            .filter(|&&k| prompt_lower.contains(k))
            .count();
        
        // Select mode based on highest score
        let scores = [
            ("code", code_score, SamplingModes::thinking_precise()),
            ("reasoning", reasoning_score, SamplingModes::instruct_reasoning()),
            ("doc", doc_score, SamplingModes::instruct_general()),
            ("creative", creative_score, SamplingModes::thinking_general()),
        ];
        
        let best = scores.iter()
            .max_by_key(|(_, score, _)| *score)
            .unwrap();
        
        if best.1 > 0 {
            log::debug!("Task classified as '{}' using {} mode", best.0, best.2.description);
            best.2.clone()
        } else {
            // Default to thinking precise for safety
            SamplingModes::thinking_precise()
        }
    }
    
    /// Get sampling mode by name
    pub fn by_name(name: &str) -> Option<SamplingMode> {
        match name {
            "thinking_general" => Some(SamplingModes::thinking_general()),
            "thinking_precise" => Some(SamplingModes::thinking_precise()),
            "instruct_general" => Some(SamplingModes::instruct_general()),
            "instruct_reasoning" => Some(SamplingModes::instruct_reasoning()),
            _ => None,
        }
    }
}

/// Convert sampling mode to vLLM extra_body parameters
impl SamplingMode {
    pub fn to_extra_body(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "temperature": self.temperature,
            "top_p": self.top_p,
            "top_k": self.top_k,
            "min_p": self.min_p,
            "presence_penalty": self.presence_penalty,
            "repetition_penalty": self.repetition_penalty,
        });
        
        // Add thinking mode parameter for Qwen3
        if self.use_thinking {
            body["chat_template_kwargs"] = serde_json::json!({
                "enable_thinking": true
            });
        }
        
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_task_classification() {
        // Code task
        let code = TaskClassifier::classify("Write a Python function to sort a list");
        assert!(code.use_thinking);
        assert_eq!(code.temperature, 0.6);
        
        // Reasoning task
        let reasoning = TaskClassifier::classify("Analyze the time complexity of this algorithm");
        assert!(!reasoning.use_thinking);
        assert_eq!(reasoning.temperature, 1.0);
        assert_eq!(reasoning.top_p, 1.0);
        
        // Doc task
        let doc = TaskClassifier::classify("Document this API endpoint");
        assert!(!doc.use_thinking);
        assert_eq!(doc.temperature, 0.7);
    }
    
    #[test]
    fn test_extra_body_generation() {
        let mode = SamplingModes::thinking_precise();
        let body = mode.to_extra_body();
        
        assert_eq!(body["temperature"], 0.6);
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
    }
}
