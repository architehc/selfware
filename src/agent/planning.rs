use anyhow::Result;

pub struct Planner;

impl Planner {
    pub fn create_plan(task: &str, context: &str) -> String {
        format!(r#"
<task>
{}
</task>

<context>
{}
</context>

Create a step-by-step plan to accomplish this task. Analyze the codebase first if needed, then determine the specific files to modify and changes to make.
"#, task, context)
    }
}
