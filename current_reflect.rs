    /// Reflect on a completed step: record lessons, update learner, inject hints
    async fn reflect_on_step(&mut self, step: usize) {
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

        // 3. Mark the plan step complete with notes
        let notes = format!("Step {} completed", step);
        self.cognitive_state
            .working_memory
            .complete_step(step, Some(notes));

        self.cognitive_state.set_phase(CyclePhase::Do);
    }

    /// Set the TUI event sender for real-time updates
    #[cfg(feature = "tui")]
    pub fn with_event_sender(
        mut self,
