/// Planner generates structured prompts for task planning
pub struct Planner;

impl Planner {
    /// Create a planning prompt with task and context
    pub fn create_plan(task: &str, context: &str) -> String {
        format!(
            r#"
         <task>
        {}
         </task>

         <context>
        {}
         </context>

         Create a step-by-step plan to accomplish this task. Analyze the codebase first if needed, then determine the specific files to modify and changes to make.
         "#,
            task, context
        )
    }

    /// Create a prompt for analyzing codebase structure
    pub fn analyze_prompt(path: &str) -> String {
        format!(
            r#"
Analyze the codebase at {} and provide:
1. Directory structure overview
2. Key files and their purposes
3. Dependencies (Cargo.toml, package.json, etc.)
4. Architecture patterns used
5. Entry points and main functionality

Be thorough but concise.
"#,
            path
        )
    }

    /// Create a prompt for code review
    pub fn review_prompt(file_path: &str, content: &str) -> String {
        format!(
            r#"
Review the following code from {}:

```
{}
```

Identify:
1. Potential bugs or issues
2. Code quality improvements
3. Security concerns
4. Performance optimizations
5. Documentation needs
"#,
            file_path, content
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/planning/planning_test.rs"]
mod tests;
