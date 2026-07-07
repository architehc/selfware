use crate::swl::types::schema::StateSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwlDocument {
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Option<DocumentMetadata>,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentDefinition>,
    #[serde(default)]
    pub workflows: BTreeMap<String, WorkflowDefinition>,
    #[serde(default)]
    pub guardrails: Vec<Guardrail>,
    #[serde(default)]
    pub telemetry: Option<TelemetryConfig>,
    #[serde(default)]
    pub dashboard: Option<DashboardConfig>,
    #[serde(default)]
    pub state: Option<StateSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentMetadata {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDefinition {
    pub model: ModelSpec,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub output_key: Option<String>,
    #[serde(default)]
    pub sub_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ModelSpec {
    Simple(String),
    Detailed(ModelConfig),
}

impl ModelSpec {
    pub fn name(&self) -> &str {
        match self {
            ModelSpec::Simple(name) => name,
            ModelSpec::Detailed(config) => config.name.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelConfig {
    #[serde(default)]
    pub provider: Option<String>,
    pub name: String,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDefinition {
    #[serde(rename = "type")]
    pub workflow_type: WorkflowType,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub map: Option<MapStage>,
    #[serde(default)]
    pub reduce: Option<ReduceStage>,
    #[serde(default)]
    pub merge: Option<AggregateStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowType {
    #[serde(rename = "sequential")]
    Sequential,
    #[serde(rename = "parallel")]
    Parallel,
    #[serde(rename = "map_reduce", alias = "map-reduce")]
    MapReduce,
    #[serde(rename = "conditional")]
    Conditional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStep {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub delegate: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub input: Option<serde_yaml::Value>,
    #[serde(default)]
    pub parallel: Option<ParallelStage>,
    #[serde(default)]
    pub guard: Option<Guardrail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParallelStage {
    #[serde(default)]
    pub branches: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapStage {
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub input: Option<serde_yaml::Value>,
    #[serde(default)]
    pub parallel: Option<ParallelStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ReduceStage {
    Code(CodeBlock),
    Aggregate(AggregateStage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregateStage {
    pub agent: String,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeBlock {
    pub language: CodeLanguage,
    #[serde(alias = "content")]
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CodeLanguage {
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "python")]
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Guardrail {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub guardrail_type: Option<String>,
    pub condition: GuardCondition,
    pub on_violation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum GuardCondition {
    Inline(String),
    Code(CodeBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub traces: Vec<String>,
    #[serde(default)]
    pub metrics: Vec<String>,
    #[serde(default)]
    pub export: Option<TelemetryExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryExport {
    #[serde(rename = "type")]
    pub export_type: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardConfig {
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub refresh: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── ModelSpec::name() ─────────────────────────────────────────────

    #[test]
    fn model_spec_simple_name_returns_inner_string() {
        let spec = ModelSpec::Simple("gpt-4".to_string());
        assert_eq!(spec.name(), "gpt-4");
    }

    #[test]
    fn model_spec_detailed_name_returns_config_name() {
        let spec = ModelSpec::Detailed(ModelConfig {
            provider: Some("openai".to_string()),
            name: "gpt-4o".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(4096),
        });
        assert_eq!(spec.name(), "gpt-4o");
    }

    #[test]
    fn model_spec_detailed_name_with_empty_name() {
        let spec = ModelSpec::Detailed(ModelConfig {
            provider: None,
            name: String::new(),
            temperature: None,
            max_tokens: None,
        });
        assert_eq!(spec.name(), "");
    }

    // ─── ModelSpec serde (untagged) ────────────────────────────────────

    #[test]
    fn model_spec_simple_round_trips_through_yaml() {
        let spec = ModelSpec::Simple("llama-3".to_string());
        let yaml = serde_yaml::to_string(&spec).unwrap();
        let back: ModelSpec = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn model_spec_detailed_round_trips_through_yaml() {
        let spec = ModelSpec::Detailed(ModelConfig {
            provider: Some("anthropic".to_string()),
            name: "claude-3".to_string(),
            temperature: Some(0.5),
            max_tokens: Some(8192),
        });
        let yaml = serde_yaml::to_string(&spec).unwrap();
        let back: ModelSpec = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn model_spec_deserializes_from_plain_string() {
        let yaml = "gpt-4\n";
        let spec: ModelSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec, ModelSpec::Simple("gpt-4".to_string()));
    }

    #[test]
    fn model_spec_deserializes_from_map() {
        let yaml = "name: test-model\ntemperature: 0.1\n";
        let spec: ModelSpec = serde_yaml::from_str(yaml).unwrap();
        match spec {
            ModelSpec::Detailed(cfg) => {
                assert_eq!(cfg.name, "test-model");
                assert_eq!(cfg.temperature, Some(0.1));
                assert_eq!(cfg.provider, None);
                assert_eq!(cfg.max_tokens, None);
            }
            _ => panic!("expected Detailed variant"),
        }
    }

    // ─── ModelConfig serde (defaults) ──────────────────────────────────

    #[test]
    fn model_config_minimal_yaml_uses_defaults() {
        let yaml = "name: mini\n";
        let cfg: ModelConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.name, "mini");
        assert_eq!(cfg.provider, None);
        assert_eq!(cfg.temperature, None);
        assert_eq!(cfg.max_tokens, None);
    }

    #[test]
    fn model_config_full_yaml_round_trips() {
        let cfg = ModelConfig {
            provider: Some("local".to_string()),
            name: "qwen".to_string(),
            temperature: Some(1.0),
            max_tokens: Some(2048),
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: ModelConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg, back);
    }

    // ─── SwlDocument serde ─────────────────────────────────────────────

    #[test]
    fn swl_document_minimal_yaml() {
        let yaml = "version: \"1.0\"\nname: test\n";
        let doc: SwlDocument = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(doc.version, "1.0");
        assert_eq!(doc.name, "test");
        assert_eq!(doc.description, None);
        assert_eq!(doc.metadata, None);
        assert!(doc.agents.is_empty());
        assert!(doc.workflows.is_empty());
        assert!(doc.guardrails.is_empty());
        assert_eq!(doc.telemetry, None);
        assert_eq!(doc.dashboard, None);
        assert_eq!(doc.state, None);
    }

    #[test]
    fn swl_document_full_round_trip() {
        let doc = SwlDocument {
            version: "2.0".to_string(),
            name: "full".to_string(),
            description: Some("A full doc".to_string()),
            metadata: Some(DocumentMetadata {
                author: Some("tester".to_string()),
                tags: vec!["a".to_string(), "b".to_string()],
            }),
            agents: BTreeMap::new(),
            workflows: BTreeMap::new(),
            guardrails: vec![],
            telemetry: None,
            dashboard: None,
            state: None,
        };
        let yaml = serde_yaml::to_string(&doc).unwrap();
        let back: SwlDocument = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn swl_document_with_agents_and_workflows_round_trip() {
        let mut agents = BTreeMap::new();
        agents.insert(
            "worker".to_string(),
            AgentDefinition {
                model: ModelSpec::Simple("m1".to_string()),
                role: Some("coder".to_string()),
                instruction: Some("do stuff".to_string()),
                tools: vec!["file_read".to_string()],
                output_key: Some("out".to_string()),
                sub_agents: vec![],
            },
        );

        let mut workflows = BTreeMap::new();
        workflows.insert(
            "w1".to_string(),
            WorkflowDefinition {
                workflow_type: WorkflowType::Sequential,
                description: Some("seq".to_string()),
                steps: vec![WorkflowStep {
                    name: Some("s1".to_string()),
                    delegate: None,
                    agent: Some("worker".to_string()),
                    action: None,
                    input: None,
                    parallel: None,
                    guard: None,
                }],
                map: None,
                reduce: None,
                merge: None,
            },
        );

        let doc = SwlDocument {
            version: "2.0".to_string(),
            name: "doc".to_string(),
            description: None,
            metadata: None,
            agents,
            workflows,
            guardrails: vec![],
            telemetry: None,
            dashboard: None,
            state: None,
        };
        let yaml = serde_yaml::to_string(&doc).unwrap();
        let back: SwlDocument = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(doc, back);
    }

    // ─── DocumentMetadata serde ────────────────────────────────────────

    #[test]
    fn document_metadata_defaults() {
        let yaml = "{}\n";
        let meta: DocumentMetadata = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(meta.author, None);
        assert!(meta.tags.is_empty());
    }

    #[test]
    fn document_metadata_round_trip() {
        let meta = DocumentMetadata {
            author: Some("alice".to_string()),
            tags: vec!["x".to_string(), "y".to_string()],
        };
        let yaml = serde_yaml::to_string(&meta).unwrap();
        let back: DocumentMetadata = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(meta, back);
    }

    // ─── AgentDefinition serde ─────────────────────────────────────────

    #[test]
    fn agent_definition_defaults_all_optional() {
        let yaml = "model: simple-model\n";
        let agent: AgentDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(agent.model.name(), "simple-model");
        assert_eq!(agent.role, None);
        assert_eq!(agent.instruction, None);
        assert!(agent.tools.is_empty());
        assert_eq!(agent.output_key, None);
        assert!(agent.sub_agents.is_empty());
    }

    #[test]
    fn agent_definition_full_round_trip() {
        let agent = AgentDefinition {
            model: ModelSpec::Detailed(ModelConfig {
                provider: Some("p".to_string()),
                name: "n".to_string(),
                temperature: Some(0.3),
                max_tokens: Some(100),
            }),
            role: Some("reviewer".to_string()),
            instruction: Some("review code".to_string()),
            tools: vec!["t1".to_string(), "t2".to_string()],
            output_key: Some("review".to_string()),
            sub_agents: vec!["sub1".to_string()],
        };
        let yaml = serde_yaml::to_string(&agent).unwrap();
        let back: AgentDefinition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(agent, back);
    }

    // ─── WorkflowType serde (rename + alias) ───────────────────────────

    #[test]
    fn workflow_type_sequential_serializes_as_sequential() {
        let yaml = serde_yaml::to_string(&WorkflowType::Sequential).unwrap();
        assert_eq!(yaml.trim(), "sequential");
    }

    #[test]
    fn workflow_type_parallel_serializes_as_parallel() {
        let yaml = serde_yaml::to_string(&WorkflowType::Parallel).unwrap();
        assert_eq!(yaml.trim(), "parallel");
    }

    #[test]
    fn workflow_type_map_reduce_serializes_as_map_reduce() {
        let yaml = serde_yaml::to_string(&WorkflowType::MapReduce).unwrap();
        assert_eq!(yaml.trim(), "map_reduce");
    }

    #[test]
    fn workflow_type_conditional_serializes_as_conditional() {
        let yaml = serde_yaml::to_string(&WorkflowType::Conditional).unwrap();
        assert_eq!(yaml.trim(), "conditional");
    }

    #[test]
    fn workflow_type_map_reduce_accepts_alias_map_dash_reduce() {
        let yaml = "map-reduce\n";
        let wt: WorkflowType = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(wt, WorkflowType::MapReduce);
    }

    #[test]
    fn workflow_type_map_reduce_accepts_primary_name() {
        let yaml = "map_reduce\n";
        let wt: WorkflowType = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(wt, WorkflowType::MapReduce);
    }

    #[test]
    fn workflow_type_invalid_value_fails() {
        let yaml = "unknown\n";
        let result: Result<WorkflowType, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    // ─── WorkflowDefinition serde ──────────────────────────────────────

    #[test]
    fn workflow_definition_type_field_renamed() {
        let yaml = "type: sequential\n";
        let wf: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(wf.workflow_type, WorkflowType::Sequential);
        assert_eq!(wf.description, None);
        assert!(wf.steps.is_empty());
        assert_eq!(wf.map, None);
        assert_eq!(wf.reduce, None);
        assert_eq!(wf.merge, None);
    }

    #[test]
    fn workflow_definition_with_steps_round_trip() {
        let wf = WorkflowDefinition {
            workflow_type: WorkflowType::Parallel,
            description: Some("desc".to_string()),
            steps: vec![
                WorkflowStep {
                    name: Some("a".to_string()),
                    delegate: Some("d".to_string()),
                    agent: None,
                    action: Some("act".to_string()),
                    input: None,
                    parallel: None,
                    guard: None,
                },
                WorkflowStep {
                    name: None,
                    delegate: None,
                    agent: Some("ag".to_string()),
                    action: None,
                    input: None,
                    parallel: Some(ParallelStage {
                        branches: vec![WorkflowStep {
                            name: Some("branch1".to_string()),
                            delegate: None,
                            agent: None,
                            action: None,
                            input: None,
                            parallel: None,
                            guard: None,
                        }],
                    }),
                    guard: None,
                },
            ],
            map: None,
            reduce: None,
            merge: None,
        };
        let yaml = serde_yaml::to_string(&wf).unwrap();
        let back: WorkflowDefinition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(wf, back);
    }

    #[test]
    fn workflow_definition_map_reduce_with_reduce_code_round_trip() {
        let wf = WorkflowDefinition {
            workflow_type: WorkflowType::MapReduce,
            description: None,
            steps: vec![],
            map: Some(MapStage {
                targets: vec!["t1".to_string(), "t2".to_string()],
                input: None,
                parallel: None,
            }),
            reduce: Some(ReduceStage::Code(CodeBlock {
                language: CodeLanguage::Rust,
                code: "fn main() {}".to_string(),
            })),
            merge: None,
        };
        let yaml = serde_yaml::to_string(&wf).unwrap();
        let back: WorkflowDefinition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(wf, back);
    }

    #[test]
    fn workflow_definition_map_reduce_with_merge_aggregate_round_trip() {
        let wf = WorkflowDefinition {
            workflow_type: WorkflowType::MapReduce,
            description: None,
            steps: vec![],
            map: Some(MapStage {
                targets: vec!["t1".to_string()],
                input: None,
                parallel: None,
            }),
            reduce: Some(ReduceStage::Aggregate(AggregateStage {
                agent: "aggregator".to_string(),
                instruction: Some("merge all".to_string()),
                inputs: vec!["t1".to_string()],
            })),
            merge: Some(AggregateStage {
                agent: "merger".to_string(),
                instruction: None,
                inputs: vec![],
            }),
        };
        let yaml = serde_yaml::to_string(&wf).unwrap();
        let back: WorkflowDefinition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(wf, back);
    }

    // ─── WorkflowStep serde ────────────────────────────────────────────

    #[test]
    fn workflow_step_empty_defaults() {
        let yaml = "{}\n";
        let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.name, None);
        assert_eq!(step.delegate, None);
        assert_eq!(step.agent, None);
        assert_eq!(step.action, None);
        assert_eq!(step.input, None);
        assert_eq!(step.parallel, None);
        assert_eq!(step.guard, None);
    }

    #[test]
    fn workflow_step_with_input_yaml_value() {
        let yaml = "agent: worker\ninput:\n  key: value\n  num: 42\n";
        let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.agent.as_deref(), Some("worker"));
        let input = step.input.expect("input should be present");
        assert_eq!(input["key"].as_str(), Some("value"));
        assert_eq!(input["num"].as_i64(), Some(42));
    }

    // ─── ParallelStage serde ───────────────────────────────────────────

    #[test]
    fn parallel_stage_empty_branches() {
        let yaml = "{}\n";
        let ps: ParallelStage = serde_yaml::from_str(yaml).unwrap();
        assert!(ps.branches.is_empty());
    }

    #[test]
    fn parallel_stage_with_branches_round_trip() {
        let ps = ParallelStage {
            branches: vec![
                WorkflowStep {
                    name: Some("b1".to_string()),
                    delegate: None,
                    agent: Some("a1".to_string()),
                    action: None,
                    input: None,
                    parallel: None,
                    guard: None,
                },
                WorkflowStep {
                    name: Some("b2".to_string()),
                    delegate: None,
                    agent: Some("a2".to_string()),
                    action: None,
                    input: None,
                    parallel: None,
                    guard: None,
                },
            ],
        };
        let yaml = serde_yaml::to_string(&ps).unwrap();
        let back: ParallelStage = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(ps, back);
    }

    // ─── MapStage serde ────────────────────────────────────────────────

    #[test]
    fn map_stage_defaults() {
        let yaml = "{}\n";
        let ms: MapStage = serde_yaml::from_str(yaml).unwrap();
        assert!(ms.targets.is_empty());
        assert_eq!(ms.input, None);
        assert_eq!(ms.parallel, None);
    }

    #[test]
    fn map_stage_full_round_trip() {
        let ms = MapStage {
            targets: vec!["a".to_string(), "b".to_string()],
            input: Some(serde_yaml::Value::String("val".to_string())),
            parallel: Some(ParallelStage {
                branches: vec![WorkflowStep {
                    name: Some("x".to_string()),
                    delegate: None,
                    agent: None,
                    action: None,
                    input: None,
                    parallel: None,
                    guard: None,
                }],
            }),
        };
        let yaml = serde_yaml::to_string(&ms).unwrap();
        let back: MapStage = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(ms, back);
    }

    // ─── ReduceStage serde (untagged) ──────────────────────────────────

    #[test]
    fn reduce_stage_code_variant_round_trip() {
        let rs = ReduceStage::Code(CodeBlock {
            language: CodeLanguage::Python,
            code: "print('hi')".to_string(),
        });
        let yaml = serde_yaml::to_string(&rs).unwrap();
        let back: ReduceStage = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(rs, back);
    }

    #[test]
    fn reduce_stage_aggregate_variant_round_trip() {
        let rs = ReduceStage::Aggregate(AggregateStage {
            agent: "agg".to_string(),
            instruction: Some("combine".to_string()),
            inputs: vec!["i1".to_string(), "i2".to_string()],
        });
        let yaml = serde_yaml::to_string(&rs).unwrap();
        let back: ReduceStage = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(rs, back);
    }

    #[test]
    fn reduce_stage_deserializes_code_from_yaml() {
        // untagged: Code has language + code, Aggregate has agent
        let yaml = "language: rust\ncode: |\n  fn x() {}\n";
        let rs: ReduceStage = serde_yaml::from_str(yaml).unwrap();
        match rs {
            ReduceStage::Code(cb) => {
                assert_eq!(cb.language, CodeLanguage::Rust);
                assert!(cb.code.contains("fn x()"));
            }
            _ => panic!("expected Code variant"),
        }
    }

    #[test]
    fn reduce_stage_deserializes_aggregate_from_yaml() {
        let yaml = "agent: myagent\ninputs: [a, b]\n";
        let rs: ReduceStage = serde_yaml::from_str(yaml).unwrap();
        match rs {
            ReduceStage::Aggregate(agg) => {
                assert_eq!(agg.agent, "myagent");
                assert_eq!(agg.inputs, vec!["a".to_string(), "b".to_string()]);
            }
            _ => panic!("expected Aggregate variant"),
        }
    }

    // ─── AggregateStage serde ──────────────────────────────────────────

    #[test]
    fn aggregate_stage_minimal_yaml() {
        let yaml = "agent: agg\n";
        let agg: AggregateStage = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(agg.agent, "agg");
        assert_eq!(agg.instruction, None);
        assert!(agg.inputs.is_empty());
    }

    #[test]
    fn aggregate_stage_full_round_trip() {
        let agg = AggregateStage {
            agent: "merger".to_string(),
            instruction: Some("merge results".to_string()),
            inputs: vec!["r1".to_string(), "r2".to_string(), "r3".to_string()],
        };
        let yaml = serde_yaml::to_string(&agg).unwrap();
        let back: AggregateStage = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(agg, back);
    }

    #[test]
    fn aggregate_stage_missing_agent_fails() {
        let yaml = "instruction: test\n";
        let result: Result<AggregateStage, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    // ─── CodeBlock serde ───────────────────────────────────────────────

    #[test]
    fn code_block_rust_round_trip() {
        let cb = CodeBlock {
            language: CodeLanguage::Rust,
            code: "fn main() { println!(\"hi\"); }".to_string(),
        };
        let yaml = serde_yaml::to_string(&cb).unwrap();
        let back: CodeBlock = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(cb, back);
    }

    #[test]
    fn code_block_python_round_trip() {
        let cb = CodeBlock {
            language: CodeLanguage::Python,
            code: "print('hi')".to_string(),
        };
        let yaml = serde_yaml::to_string(&cb).unwrap();
        let back: CodeBlock = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(cb, back);
    }

    #[test]
    fn code_block_accepts_content_alias() {
        let yaml = "language: python\ncontent: print(1)\n";
        let cb: CodeBlock = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cb.language, CodeLanguage::Python);
        assert_eq!(cb.code, "print(1)");
    }

    #[test]
    fn code_block_accepts_code_field() {
        let yaml = "language: rust\ncode: fn x()\n";
        let cb: CodeBlock = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cb.language, CodeLanguage::Rust);
        assert_eq!(cb.code, "fn x()");
    }

    #[test]
    fn code_block_missing_language_fails() {
        let yaml = "code: fn x()\n";
        let result: Result<CodeBlock, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn code_block_missing_code_fails() {
        let yaml = "language: rust\n";
        let result: Result<CodeBlock, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    // ─── CodeLanguage serde ────────────────────────────────────────────

    #[test]
    fn code_language_rust_serializes_correctly() {
        let yaml = serde_yaml::to_string(&CodeLanguage::Rust).unwrap();
        assert_eq!(yaml.trim(), "rust");
    }

    #[test]
    fn code_language_python_serializes_correctly() {
        let yaml = serde_yaml::to_string(&CodeLanguage::Python).unwrap();
        assert_eq!(yaml.trim(), "python");
    }

    #[test]
    fn code_language_invalid_value_fails() {
        let yaml = "javascript\n";
        let result: Result<CodeLanguage, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    // ─── Guardrail serde ───────────────────────────────────────────────

    #[test]
    fn guardrail_inline_condition_round_trip() {
        let g = Guardrail {
            name: Some("safety".to_string()),
            guardrail_type: Some("output".to_string()),
            condition: GuardCondition::Inline("x > 10".to_string()),
            on_violation: "abort".to_string(),
        };
        let yaml = serde_yaml::to_string(&g).unwrap();
        let back: Guardrail = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(g, back);
    }

    #[test]
    fn guardrail_code_condition_round_trip() {
        let g = Guardrail {
            name: None,
            guardrail_type: None,
            condition: GuardCondition::Code(CodeBlock {
                language: CodeLanguage::Python,
                code: "check()".to_string(),
            }),
            on_violation: "warn".to_string(),
        };
        let yaml = serde_yaml::to_string(&g).unwrap();
        let back: Guardrail = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(g, back);
    }

    #[test]
    fn guardrail_minimal_yaml_requires_condition_and_on_violation() {
        let yaml = "condition: \"true\"\non_violation: skip\n";
        let g: Guardrail = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(g.name, None);
        assert_eq!(g.guardrail_type, None);
        match g.condition {
            GuardCondition::Inline(s) => assert_eq!(s, "true"),
            _ => panic!("expected Inline condition"),
        }
        assert_eq!(g.on_violation, "skip");
    }

    #[test]
    fn guardrail_missing_on_violation_fails() {
        let yaml = "condition: \"true\"\n";
        let result: Result<Guardrail, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn guardrail_type_field_renamed_to_type_in_yaml() {
        let yaml = "name: g1\ntype: input\ncondition: \"false\"\non_violation: block\n";
        let g: Guardrail = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(g.guardrail_type, Some("input".to_string()));
        assert_eq!(g.name, Some("g1".to_string()));
    }

    // ─── GuardCondition serde (untagged) ───────────────────────────────

    #[test]
    fn guard_condition_inline_from_string() {
        let yaml = "some condition\n";
        let gc: GuardCondition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(gc, GuardCondition::Inline("some condition".to_string()));
    }

    #[test]
    fn guard_condition_code_from_map() {
        let yaml = "language: rust\ncode: fn check() {}\n";
        let gc: GuardCondition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            gc,
            GuardCondition::Code(CodeBlock {
                language: CodeLanguage::Rust,
                code: "fn check() {}".to_string(),
            })
        );
    }

    // ─── TelemetryConfig serde ─────────────────────────────────────────

    #[test]
    fn telemetry_config_empty_defaults() {
        let yaml = "{}\n";
        let tc: TelemetryConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(tc.enabled, None);
        assert!(tc.traces.is_empty());
        assert!(tc.metrics.is_empty());
        assert_eq!(tc.export, None);
    }

    #[test]
    fn telemetry_config_full_round_trip() {
        let tc = TelemetryConfig {
            enabled: Some(true),
            traces: vec!["t1".to_string()],
            metrics: vec!["m1".to_string(), "m2".to_string()],
            export: Some(TelemetryExport {
                export_type: "file".to_string(),
                path: Some("/tmp/trace.json".to_string()),
            }),
        };
        let yaml = serde_yaml::to_string(&tc).unwrap();
        let back: TelemetryConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(tc, back);
    }

    #[test]
    fn telemetry_export_type_field_renamed() {
        let yaml = "type: otlp\npath: /logs\n";
        let te: TelemetryExport = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(te.export_type, "otlp");
        assert_eq!(te.path, Some("/logs".to_string()));
    }

    #[test]
    fn telemetry_export_missing_type_fails() {
        let yaml = "path: /logs\n";
        let result: Result<TelemetryExport, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    // ─── DashboardConfig serde ─────────────────────────────────────────

    #[test]
    fn dashboard_config_empty_defaults() {
        let yaml = "{}\n";
        let dc: DashboardConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(dc.layout, None);
        assert_eq!(dc.refresh, None);
    }

    #[test]
    fn dashboard_config_round_trip() {
        let dc = DashboardConfig {
            layout: Some("grid".to_string()),
            refresh: Some("5s".to_string()),
        };
        let yaml = serde_yaml::to_string(&dc).unwrap();
        let back: DashboardConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(dc, back);
    }

    // ─── Guardrail in WorkflowStep ─────────────────────────────────────

    #[test]
    fn workflow_step_with_guard_round_trip() {
        let step = WorkflowStep {
            name: Some("guarded".to_string()),
            delegate: None,
            agent: Some("a".to_string()),
            action: None,
            input: None,
            parallel: None,
            guard: Some(Guardrail {
                name: Some("g".to_string()),
                guardrail_type: Some("pre".to_string()),
                condition: GuardCondition::Inline("ready".to_string()),
                on_violation: "stop".to_string(),
            }),
        };
        let yaml = serde_yaml::to_string(&step).unwrap();
        let back: WorkflowStep = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(step, back);
        assert!(back.guard.is_some());
    }

    // ─── Nested parallel branches round trip ───────────────────────────

    #[test]
    fn nested_parallel_branches_round_trip() {
        let step = WorkflowStep {
            name: Some("outer".to_string()),
            delegate: None,
            agent: None,
            action: None,
            input: None,
            parallel: Some(ParallelStage {
                branches: vec![WorkflowStep {
                    name: Some("inner".to_string()),
                    delegate: None,
                    agent: None,
                    action: None,
                    input: None,
                    parallel: Some(ParallelStage {
                        branches: vec![WorkflowStep {
                            name: Some("leaf".to_string()),
                            delegate: None,
                            agent: Some("leaf_agent".to_string()),
                            action: None,
                            input: None,
                            parallel: None,
                            guard: None,
                        }],
                    }),
                    guard: None,
                }],
            }),
            guard: None,
        };
        let yaml = serde_yaml::to_string(&step).unwrap();
        let back: WorkflowStep = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(step, back);

        // Verify the nesting survived
        let outer_parallel = back.parallel.expect("outer parallel");
        assert_eq!(outer_parallel.branches.len(), 1);
        let inner = &outer_parallel.branches[0];
        let inner_parallel = inner.parallel.as_ref().expect("inner parallel");
        assert_eq!(inner_parallel.branches.len(), 1);
        assert_eq!(inner_parallel.branches[0].name, Some("leaf".to_string()));
    }
}
