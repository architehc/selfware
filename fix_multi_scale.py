import re

with open('src/cognitive/state.rs', 'r') as f:
    content = f.read()

# Add new structs for Multi-Scale Planning
multi_scale_structs = """
/// Strategic level goal spanning multiple sessions/days
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategicGoal {
    pub id: String,
    pub description: String,
    pub criteria_for_success: Vec<String>,
    pub status: StepStatus,
    pub tactical_plans: Vec<TacticalPlan>,
}

/// Tactical level plan spanning hours/multiple operational tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalPlan {
    pub id: String,
    pub description: String,
    pub status: StepStatus,
    pub operational_tasks: Vec<String>, // Task IDs
}

/// Operational level plan (current task execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalPlan {
    pub task_id: String,
    pub steps: Vec<PlanStep>,
}
"""

# Insert before PlanStep
content = content.replace('/// A step in the plan', multi_scale_structs + '\n/// A step in the plan')

# Add to CognitiveState
content = content.replace(
    'pub struct CognitiveState {',
    'pub struct CognitiveState {\n    pub strategic_goals: Vec<StrategicGoal>,\n    pub active_tactical_plan: Option<TacticalPlan>,\n    pub active_operational_plan: Option<OperationalPlan>,'
)

# Update CognitiveState::new()
content = content.replace(
    'phase: CyclePhase::Plan,',
    'phase: CyclePhase::Plan,\n            strategic_goals: Vec::new(),\n            active_tactical_plan: None,\n            active_operational_plan: None,'
)

with open('src/cognitive/state.rs', 'w') as f:
    f.write(content)
