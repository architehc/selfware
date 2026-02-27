import re

with open('src/agent/mod.rs', 'r') as f:
    content = f.read()

replacement = """
        // 3. LLM Functional Reflection (Every 5 steps)
        if step > 0 && step % 5 == 0 {
            info!("Triggering functional reflection for step {}", step);
            // We use the LLM to analyze what was learned
            let reflection_prompt = format!(
                "You have just completed step {}. Reflect on the last 5 steps.\n\
                What did you learn? What would you do differently? What surprised you?\n\
                Be concise. Output your reflection as a single paragraph.",
                step
            );
            
            // This is a simplified call without full tool parsing for brevity in the loop
            let mut messages = self.messages.clone();
            messages.push(crate::api::types::Message {
                role: crate::api::types::Role::User,
                content: reflection_prompt,
                name: None,
            });
            
            if let Ok(response) = self.client.send_message(
                &messages,
                &self.tools,
                None,
                crate::api::types::ThinkingMode::Disabled,
            ).await {
                let text = response.content.clone().unwrap_or_default();
                if !text.is_empty() {
                    self.cognitive_state.episodic_memory.record_lesson(
                        crate::cognitive::LessonCategory::Architecture,
                        &format!("Reflection at step {}: {}", step, text),
                        None,
                    );
                    self.cognitive_state.working_memory.add_fact(&format!("Reflection (Step {}): {}", step, text));
                }
            }
        }

        // 4. Mark the plan step complete with notes
"""

content = content.replace('// 3. Mark the plan step complete with notes', replacement)

with open('src/agent/mod.rs', 'w') as f:
    f.write(content)
