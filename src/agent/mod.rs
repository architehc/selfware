use anyhow::{Context, Result};
use colored::*;
use serde_json::Value;
use tracing::{info, warn, debug};

use crate::api::{KimiClient, ThinkingMode};
use crate::api::types::Message;
use crate::config::Config;
use crate::memory::AgentMemory;
use crate::safety::SafetyChecker;
use crate::tools::ToolRegistry;

pub mod context;
pub mod loop_control;

use context::ContextCompressor;
use loop_control::{AgentLoop, AgentState};

pub struct Agent {
    client: KimiClient,
    tools: ToolRegistry,
    #[allow(dead_code)]  // Will be used for persistent memory later
    memory: AgentMemory,
    safety: SafetyChecker,
    config: Config,
    loop_control: AgentLoop,
    messages: Vec<Message>,
    compressor: ContextCompressor,
}


impl Agent {
    pub async fn new(config: Config) -> Result<Self> {
        let client = KimiClient::new(&config)?;
        let tools = ToolRegistry::new();
        let memory = AgentMemory::new(&config)?;
        let safety = SafetyChecker::new(&config.safety);
        let loop_control = AgentLoop::new(config.agent.max_iterations);
        let compressor = ContextCompressor::new(config.max_tokens);
        
        let system_prompt = r#"You are Kimi, an expert software engineering AI assistant running in an agentic harness. You have access to tools for file operations, git, and shell execution.

When given a task:
1. First analyze the codebase to understand structure
2. Create a plan with specific steps
3. Execute tools to accomplish the task
4. Verify each step succeeded before proceeding
5. If you encounter errors, analyze and retry with a different approach

Always think step by step before taking action. When editing files, ensure you have sufficient context (3-5 lines) to make unique matches."#;

        let messages = vec![Message::system(system_prompt)];
        
        Ok(Self {
            client,
            tools,
            memory,
            safety,
            config,
            loop_control,
            messages,
            compressor,
        })
    }

    pub async fn run_task(&mut self, task: &str) -> Result<()> {
        println!("{}", "🦊 Kimi Agent starting task...".bright_cyan());
        println!("Task: {}", task.bright_white());
        
        // Add user message
        self.messages.push(Message::user(task));
        
        let mut iteration = 0;
        
        while let Some(state) = self.loop_control.next_state() {
            match state {
                AgentState::Planning => {
                    println!("{}", "📋 Planning...".bright_yellow());
                    self.plan().await?;
                    self.loop_control.set_state(AgentState::Executing { step: 0 });
                }
                AgentState::Executing { step } => {
                    println!("{} {}", format!("📝 Step {}", step + 1).bright_blue(), "Executing...");
                    match self.execute_step().await {
                        Ok(completed) => {
                            if completed {
                                println!("{}", "✅ Task completed!".bright_green());
                                return Ok(());
                            }
                            self.loop_control.increment_step();
                        }
                        Err(e) => {
                            warn!("Step failed: {}", e);
                            self.loop_control.set_state(AgentState::ErrorRecovery { error: e.to_string() });
                        }
                    }
                }
                AgentState::ErrorRecovery { error } => {
                    println!("{} {}", "⚠️ Recovering from error:".bright_red(), error);
                    self.messages.push(Message::user(format!(
                        "The previous action failed with error: {}. Please try a different approach.",
                        error
                    )));
                    self.loop_control.set_state(AgentState::Executing { step: self.loop_control.current_step() });
                }
                AgentState::Completed => {
                    println!("{}", "✅ Task completed successfully!".bright_green());
                    return Ok(());
                }
                AgentState::Failed { reason } => {
                    println!("{} {}", "❌ Task failed:".bright_red(), reason);
                    anyhow::bail!("Agent failed: {}", reason);
                }
            }
            
            iteration += 1;
            if iteration > self.config.agent.max_iterations {
                anyhow::bail!("Max iterations reached");
            }
        }
        
        Ok(())
    }

    async fn plan(&mut self) -> Result<()> {
        let tools = self.tools.definitions();
        
        let response = self.client.chat(
            self.messages.clone(),
            Some(tools),
            ThinkingMode::Enabled,
        ).await?;
        
        let choice = response.choices.into_iter().next()
            .context("No response from model")?;
            
        let assistant_msg = choice.message;
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: assistant_msg.content.clone(),
            reasoning_content: assistant_msg.reasoning_content,
            tool_calls: assistant_msg.tool_calls,
            tool_call_id: None,
            name: None,
        });
        
        Ok(())
    }

    async fn execute_step(&mut self) -> Result<bool> {
        let tools = self.tools.definitions();
        
                // Check if we need compression
        if self.compressor.should_compress(&self.messages) {
            info!("Context compression triggered");
            match self.compressor.compress(&self.client, &self.messages).await {
                Ok(compressed) => {
                    self.messages = compressed;
                }
                Err(e) => {
                    warn!("Compression failed, using hard limit: {}", e);
                    self.messages = self.compressor.hard_compress(&self.messages);
                }
            }
        }

        let response = self.client.chat(
            self.messages.clone(),
            Some(tools),
            ThinkingMode::Enabled,
        ).await?;
        
        let choice = response.choices.into_iter().next()
            .context("No response from model")?;
            
        let message = choice.message;
        
        if let Some(reasoning) = &message.reasoning_content {
            println!("{} {}", "Thinking:".dimmed(), reasoning.dimmed());
        }
        
        if let Some(calls) = &message.tool_calls {
            self.messages.push(Message {
                role: "assistant".to_string(),
                content: message.content.clone(),
                reasoning_content: None,
                tool_calls: Some(calls.clone()),
                tool_call_id: None,
                name: None,
            });
            
            for call in calls {
                println!("{} Calling tool: {}", "🔧".bright_blue(), call.function.name.bright_cyan());
                
                if let Err(e) = self.safety.check_tool_call(call) {
                    let error_msg = format!("Safety check failed: {}", e);
                    println!("{} {}", "🚫".bright_red(), error_msg);
                    self.messages.push(Message::tool(error_msg, &call.id));
                    continue;
                }
                
                let args: Value = serde_json::from_str(&call.function.arguments)?;
                debug!("Tool arguments: {}", args);
                
                let result = match self.tools.get(&call.function.name) {
                    Some(tool) => {
                        match tool.execute(args).await {
                            Ok(result) => {
                                println!("{} Tool succeeded", "✓".bright_green());
                                serde_json::to_string(&result)?
                            }
                            Err(e) => {
                                println!("{} Tool failed: {}", "✗".bright_red(), e);
                                format!("Error: {}", e)
                            }
                        }
                    }
                    None => format!("Error: Unknown tool {}", call.function.name),
                };
                
                self.messages.push(Message::tool(result, &call.id));
            }
            
            Ok(false)
        } else {
            println!("{} {}", "Final answer:".bright_green(), message.content);
            self.messages.push(Message {
                role: "assistant".to_string(),
                content: message.content,
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
            Ok(true)
        }
    }

    pub async fn interactive(&mut self) -> Result<()> {
        use std::io::{self, Write};
        
        println!("{}", "🦊 Kimi Agent Interactive Mode".bright_cyan());
        println!("Type 'exit' to quit, '/help' for commands");
        
        loop {
            print!("> ");
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();
            
            if input == "exit" {
                break;
            }
            
            if input == "/help" {
                println!("Commands:");
                println!("  /help     - Show this help");
                println!("  /status   - Show agent status");
                println!("  /clear    - Clear conversation history");
                println!("  /tools    - List available tools");
                println!("  exit      - Exit interactive mode");
                continue;
            }
            
            if input == "/status" {
                println!("Messages in context: {}", self.messages.len());
                println!("Current step: {}", self.loop_control.current_step());
                continue;
            }
            
            if input == "/clear" {
                self.messages.retain(|m| m.role == "system");
                println!("Conversation cleared (system prompt retained)");
                continue;
            }
            
            if input == "/tools" {
                for tool in self.tools.list() {
                    println!("  - {}: {}", tool.name(), tool.description());
                }
                continue;
            }
            
            match self.run_task(input).await {
                Ok(_) => {}
                Err(e) => println!("{} Error: {}", "❌".bright_red(), e),
            }
        }
        
        Ok(())
    }

    pub async fn analyze(&mut self, path: &str) -> Result<()> {
        let task = format!("Analyze the codebase at {} and provide a comprehensive summary of its structure, key files, dependencies, and architecture.", path);
        self.run_task(&task).await
    }
}
