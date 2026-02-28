use tracing::info;

use super::*;

impl Agent {
    pub(super) fn infer_task_type(task: &str) -> &'static str {
        let task_lower = task.to_lowercase();
        if task_lower.contains("review") {
            "code_review"
        } else if task_lower.contains("test") {
            "testing"
        } else if task_lower.contains("refactor") {
            "refactor"
        } else if task_lower.contains("fix") || task_lower.contains("bug") {
            "bug_fix"
        } else if task_lower.contains("document") || task_lower.contains("readme") {
            "documentation"
        } else {
            "general"
        }
    }

    pub(super) fn classify_error_type(error: &str) -> &'static str {
        let lower = error.to_lowercase();
        if lower.contains("timeout") || lower.contains("timed out") {
            "timeout"
        } else if lower.contains("permission") || lower.contains("denied") {
            "permission"
        } else if lower.contains("safety") || lower.contains("blocked") {
            "safety"
        } else if lower.contains("json") || lower.contains("parse") || lower.contains("invalid") {
            "parsing"
        } else if lower.contains("network") || lower.contains("connection") {
            "network"
        } else {
            "execution"
        }
    }

    pub(super) fn outcome_quality(outcome: Outcome) -> f32 {
        match outcome {
            Outcome::Success => 1.0,
            Outcome::Partial => 0.65,
            Outcome::Failure => 0.0,
            Outcome::Abandoned => 0.2,
        }
    }

    pub(super) fn learning_context(&self) -> &str {
        if self.current_task_context.is_empty() {
            "general"
        } else {
            &self.current_task_context
        }
    }

    pub(super) fn start_learning_session(&mut self, session_id: &str, task_context: &str) {
        self.current_task_context = task_context.to_string();
        self.self_improvement.start_session(session_id);
    }

    pub(super) fn record_task_outcome(
        &mut self,
        task_prompt: &str,
        outcome: Outcome,
        error: Option<&str>,
    ) {
        let task_type = Self::infer_task_type(task_prompt);
        self.self_improvement.record_prompt(
            task_prompt,
            task_type,
            outcome,
            Self::outcome_quality(outcome),
        );
        self.self_improvement.record_task(outcome.is_positive());

        if let Some(err) = error {
            self.self_improvement.record_error(
                err,
                Self::classify_error_type(err),
                self.learning_context(),
                "task_execution",
                None,
            );
        }

        self.self_improvement.end_session(None);
    }

    pub(super) fn build_learning_hint(&self, task_prompt: &str) -> Option<String> {
        if task_prompt.trim().is_empty() {
            return None;
        }

        let mut hints: Vec<String> = Vec::new();

        let preferred_tools: Vec<String> = self
            .self_improvement
            .best_tools_for(task_prompt)
            .into_iter()
            .filter(|(_, score)| *score >= 0.6)
            .take(3)
            .map(|(tool, score)| format!("{} ({:.0}% confidence)", tool, score * 100.0))
            .collect();
        if !preferred_tools.is_empty() {
            hints.push(format!(
                "Prefer previously effective tools: {}.",
                preferred_tools.join(", ")
            ));
        }

        let warnings = self
            .self_improvement
            .check_for_errors("task_execution", task_prompt);
        if let Some(warning) = warnings.into_iter().next().filter(|w| w.likelihood >= 0.6) {
            hints.push(format!(
                "Avoid recurring {} pattern (likelihood {:.0}%).",
                warning.error_type,
                warning.likelihood * 100.0
            ));
            if !warning.prevention.is_empty() {
                hints.push(format!(
                    "Prevention guidance: {}.",
                    warning
                        .prevention
                        .into_iter()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }

        if hints.is_empty() {
            None
        } else {
            Some(format!(
                "Self-improvement guidance from prior outcomes:\n- {}",
                hints.join("\n- ")
            ))
        }
    }

    /// Reflect on a completed step: record lessons, update learner, inject hints
    pub(super) async fn reflect_on_step(&mut self, step: usize) {
        self.cognitive_state.set_phase(CyclePhase::Reflect);

        // 1. Check for verification failures in the last step and record lessons
        if let Some(ref checkpoint) = self.current_checkpoint {
            let step_errors: Vec<_> = checkpoint
                .errors
                .iter()
                .filter(|e| e.step == step)
                .collect();
            for error in &step_errors {
                if error.recovered {
                    self.cognitive_state.episodic_memory.what_worked(
                        "error_recovery",
                        &format!("Step {}: recovered from: {}", step, error.error),
                    );
                    // Record recovery strategy in improvement engine
                    self.self_improvement.record_error(
                        &error.error,
                        "step_error",
                        self.learning_context(),
                        &format!("step_{}", step),
                        Some("automatic_recovery".to_string()),
                    );
                } else {
                    self.cognitive_state.episodic_memory.what_failed(
                        "step_execution",
                        &format!("Step {}: unrecovered error: {}", step, error.error),
                    );
                }
            }
        }

        // 2. Query tool learner for recommendations and inject hint into working memory
        let context = self.current_task_context.clone();
        let best_tools = self.self_improvement.best_tools_for(&context);
        if let Some((tool, score)) = best_tools.first() {
            if *score >= 0.7 {
                let hint = format!(
                    "Based on learning: tool '{}' has {:.0}% effectiveness for this context",
                    tool,
                    score * 100.0
                );
                self.cognitive_state.working_memory.add_fact(&hint);
            }
        }

        // 3. LLM Functional Reflection (Every 5 steps)
        if step > 0 && step.is_multiple_of(5) {
            info!("Triggering functional reflection for step {}", step);
            let reflection_prompt = format!(
                "You have just completed step {}. Reflect on the last 5 steps.
                What did you learn? What would you do differently? What surprised you?
                Be concise. Output your reflection as a single paragraph.",
                step
            );

            let mut messages = self.messages.clone();
            messages.push(crate::api::types::Message::user(reflection_prompt));

            if let Ok(response) = self
                .client
                .chat(messages, None, crate::api::ThinkingMode::Disabled)
                .await
            {
                if let Some(choice) = response.choices.first() {
                    let text = choice.message.content.clone();
                    if !text.is_empty() {
                        let lesson = crate::cognitive::Lesson {
                            category: crate::cognitive::LessonCategory::Discovery,
                            content: format!("Reflection at step {}: {}", step, text),
                            context: "".to_string(),
                            tags: vec!["reflection".to_string()],
                            timestamp: chrono::Utc::now(),
                        };
                        self.cognitive_state.episodic_memory.record_lesson(lesson);
                        self.cognitive_state
                            .working_memory
                            .add_fact(&format!("Reflection (Step {}): {}", step, text));
                    }
                }
            }
        }

        // 4. Mark the plan step complete with notes
        let notes = format!("Step {} completed", step);
        self.cognitive_state
            .working_memory
            .complete_step(step, Some(notes));
        self.cognitive_state
            .complete_operational_step(step, Some(format!("Step {} completed", step)));

        self.cognitive_state.set_phase(CyclePhase::Do);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::self_improvement::Outcome;

    // =========================================================================
    // infer_task_type — exhaustive branch coverage + edge cases
    // =========================================================================

    #[test]
    fn test_infer_task_type_code_review() {
        assert_eq!(Agent::infer_task_type("Please review this PR"), "code_review");
        assert_eq!(Agent::infer_task_type("Code review needed"), "code_review");
        assert_eq!(Agent::infer_task_type("REVIEW the changes"), "code_review");
    }

    #[test]
    fn test_infer_task_type_testing() {
        assert_eq!(Agent::infer_task_type("Write tests for module"), "testing");
        assert_eq!(Agent::infer_task_type("Add unit test coverage"), "testing");
        assert_eq!(Agent::infer_task_type("Run TEST suite"), "testing");
    }

    #[test]
    fn test_infer_task_type_refactor() {
        assert_eq!(Agent::infer_task_type("Refactor the parser"), "refactor");
        assert_eq!(Agent::infer_task_type("REFACTORING needed"), "refactor");
    }

    #[test]
    fn test_infer_task_type_bug_fix() {
        assert_eq!(Agent::infer_task_type("Fix this bug"), "bug_fix");
        assert_eq!(Agent::infer_task_type("There is a bug in login"), "bug_fix");
        assert_eq!(Agent::infer_task_type("FIX compilation error"), "bug_fix");
    }

    #[test]
    fn test_infer_task_type_documentation() {
        assert_eq!(Agent::infer_task_type("Document the API"), "documentation");
        assert_eq!(Agent::infer_task_type("Update README"), "documentation");
        assert_eq!(Agent::infer_task_type("Write documentation for new feature"), "documentation");
        assert_eq!(Agent::infer_task_type("Update the readme file"), "documentation");
    }

    #[test]
    fn test_infer_task_type_general_fallback() {
        assert_eq!(Agent::infer_task_type("Build the new feature"), "general");
        assert_eq!(Agent::infer_task_type("Deploy to production"), "general");
        assert_eq!(Agent::infer_task_type(""), "general");
    }

    #[test]
    fn test_infer_task_type_priority_order() {
        // "review" takes priority over "test" when both are present
        assert_eq!(Agent::infer_task_type("Review the test changes"), "code_review");
        // "test" takes priority over "refactor"
        assert_eq!(Agent::infer_task_type("Test after refactor"), "testing");
        // "refactor" takes priority over "fix"
        assert_eq!(Agent::infer_task_type("Refactor to fix the issue"), "refactor");
    }

    // =========================================================================
    // classify_error_type — exhaustive branch coverage + edge cases
    // =========================================================================

    #[test]
    fn test_classify_error_timeout() {
        assert_eq!(Agent::classify_error_type("request timed out"), "timeout");
        assert_eq!(Agent::classify_error_type("Connection timeout after 30s"), "timeout");
        assert_eq!(Agent::classify_error_type("Operation TIMED OUT"), "timeout");
    }

    #[test]
    fn test_classify_error_permission() {
        assert_eq!(Agent::classify_error_type("permission denied"), "permission");
        assert_eq!(Agent::classify_error_type("Access denied for path /root"), "permission");
        assert_eq!(Agent::classify_error_type("PERMISSION error"), "permission");
    }

    #[test]
    fn test_classify_error_safety() {
        assert_eq!(Agent::classify_error_type("Safety check failed"), "safety");
        assert_eq!(Agent::classify_error_type("Operation blocked by policy"), "safety");
        assert_eq!(Agent::classify_error_type("BLOCKED by firewall"), "safety");
    }

    #[test]
    fn test_classify_error_parsing() {
        assert_eq!(Agent::classify_error_type("Invalid JSON in response"), "parsing");
        assert_eq!(Agent::classify_error_type("Failed to parse config"), "parsing");
        assert_eq!(Agent::classify_error_type("JSON decode error"), "parsing");
        assert_eq!(Agent::classify_error_type("invalid syntax at line 5"), "parsing");
    }

    #[test]
    fn test_classify_error_network() {
        assert_eq!(Agent::classify_error_type("Network unreachable"), "network");
        assert_eq!(Agent::classify_error_type("Connection refused"), "network");
        assert_eq!(Agent::classify_error_type("NETWORK error"), "network");
    }

    #[test]
    fn test_classify_error_execution_fallback() {
        assert_eq!(Agent::classify_error_type("unknown error occurred"), "execution");
        assert_eq!(Agent::classify_error_type("segfault"), "execution");
        assert_eq!(Agent::classify_error_type(""), "execution");
    }

    #[test]
    fn test_classify_error_priority_order() {
        // "timeout" takes priority over "network" when both keywords present
        assert_eq!(
            Agent::classify_error_type("network connection timeout"),
            "timeout"
        );
        // "permission" takes priority over "safety"
        assert_eq!(
            Agent::classify_error_type("permission denied: safety policy"),
            "permission"
        );
    }

    // =========================================================================
    // outcome_quality — all variants
    // =========================================================================

    #[test]
    fn test_outcome_quality_all_variants() {
        assert_eq!(Agent::outcome_quality(Outcome::Success), 1.0);
        assert_eq!(Agent::outcome_quality(Outcome::Partial), 0.65);
        assert_eq!(Agent::outcome_quality(Outcome::Failure), 0.0);
        assert_eq!(Agent::outcome_quality(Outcome::Abandoned), 0.2);
    }

    #[test]
    fn test_outcome_quality_ordering() {
        assert!(Agent::outcome_quality(Outcome::Success) > Agent::outcome_quality(Outcome::Partial));
        assert!(Agent::outcome_quality(Outcome::Partial) > Agent::outcome_quality(Outcome::Abandoned));
        assert!(Agent::outcome_quality(Outcome::Abandoned) > Agent::outcome_quality(Outcome::Failure));
    }

    // =========================================================================
    // Outcome enum methods
    // =========================================================================

    #[test]
    fn test_outcome_is_positive() {
        assert!(Outcome::Success.is_positive());
        assert!(Outcome::Partial.is_positive());
        assert!(!Outcome::Failure.is_positive());
        assert!(!Outcome::Abandoned.is_positive());
    }

    #[test]
    fn test_outcome_score() {
        assert_eq!(Outcome::Success.score(), 1.0);
        assert_eq!(Outcome::Partial.score(), 0.5);
        assert_eq!(Outcome::Failure.score(), 0.0);
        assert_eq!(Outcome::Abandoned.score(), 0.0);
    }

    // =========================================================================
    // build_learning_hint edge cases (via mock Agent)
    // =========================================================================

    #[tokio::test]
    async fn test_build_learning_hint_empty_prompt_returns_none() {
        let server = crate::testing::mock_api::MockLlmServer::builder()
            .with_response("done")
            .build()
            .await;

        let config = crate::config::Config {
            endpoint: format!("{}/v1", server.url()),
            model: "mock".to_string(),
            agent: crate::config::AgentConfig {
                max_iterations: 5,
                step_timeout_secs: 5,
                streaming: false,
                native_function_calling: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let agent = Agent::new(config).await.unwrap();

        // Empty prompt should return None
        assert!(agent.build_learning_hint("").is_none());
        assert!(agent.build_learning_hint("   ").is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_build_learning_hint_no_data_returns_none() {
        let server = crate::testing::mock_api::MockLlmServer::builder()
            .with_response("done")
            .build()
            .await;

        let config = crate::config::Config {
            endpoint: format!("{}/v1", server.url()),
            model: "mock".to_string(),
            agent: crate::config::AgentConfig {
                max_iterations: 5,
                step_timeout_secs: 5,
                streaming: false,
                native_function_calling: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let agent = Agent::new(config).await.unwrap();

        // Fresh engine with no recorded data should return None for any prompt
        let result = agent.build_learning_hint("Write a new parser");
        // With a fresh SelfImprovementEngine, there are no preferred tools or warnings
        assert!(result.is_none());

        server.stop().await;
    }

    // =========================================================================
    // learning_context
    // =========================================================================

    #[tokio::test]
    async fn test_learning_context_defaults_to_general() {
        let server = crate::testing::mock_api::MockLlmServer::builder()
            .with_response("done")
            .build()
            .await;

        let config = crate::config::Config {
            endpoint: format!("{}/v1", server.url()),
            model: "mock".to_string(),
            agent: crate::config::AgentConfig {
                max_iterations: 5,
                step_timeout_secs: 5,
                streaming: false,
                native_function_calling: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let agent = Agent::new(config).await.unwrap();

        // Default empty context returns "general"
        assert_eq!(agent.learning_context(), "general");

        server.stop().await;
    }
}
