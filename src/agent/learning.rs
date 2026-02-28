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
