use super::*;
use crate::consolidation::{ConsolidationConfig, ConsolidationEngine};
use chrono::Duration;
use tempfile::tempdir;

#[test]
fn test_distilled_skill_to_markdown_and_parse_roundtrip() {
    let skill = DistilledSkill {
        name: "test_mitigation".to_string(),
        skill_type: DistilledSkillType::ErrorMitigation,
        description: "Test mitigation description".to_string(),
        tools: vec!["cargo_check".to_string(), "file_edit".to_string()],
        triggers: vec!["test error signature".to_string()],
        content: "# Instructions\nExecute the steps carefully.\n".to_string(),
    };

    let md = skill.to_markdown();
    assert!(md.starts_with("---\n"));
    assert!(md.contains("name: test_mitigation"));
    assert!(md.contains("category: error_mitigation"));
    assert!(md.contains("verified: false"));
    assert!(md.contains("cargo_check"));

    // Verify compatibility with Skill::from_markdown
    let parsed = Skill::from_markdown(&md).expect("should parse successfully");
    assert_eq!(parsed.name, "test_mitigation");
    assert_eq!(parsed.description, "Test mitigation description");
    assert_eq!(parsed.tools, vec!["cargo_check", "file_edit"]);
    assert!(!parsed.verified);
    assert!(parsed.content.contains("Execute the steps carefully"));

    // Test that colons in description are handled safely by serde_yaml without breaking YAML
    let skill_with_colon = DistilledSkill {
        name: "test_colon".to_string(),
        skill_type: DistilledSkillType::StandardOperatingProcedure,
        description: "Fix bug: null pointer in parser: line 42".to_string(),
        tools: vec![],
        triggers: vec![],
        content: "Fix steps.\n".to_string(),
    };
    let md_colon = skill_with_colon.to_markdown();
    let parsed_colon = Skill::from_markdown(&md_colon).expect("colon in description must parse");
    assert_eq!(parsed_colon.name, "test_colon");
    assert_eq!(
        parsed_colon.description,
        "Fix bug: null pointer in parser: line 42"
    );
    assert!(!parsed_colon.verified);
}

#[test]
fn test_skill_ledger_entry_compute_utility() {
    let now = Utc::now();
    let mut entry = SkillLedgerEntry::new("my_skill", "sop");
    assert_eq!(entry.invocations, 0);
    assert_eq!(entry.success_count, 0);

    let initial_score = entry.compute_utility(now, 0.01);
    assert!(initial_score > 0.0);

    // After 5 successful invocations, utility must increase
    entry.invocations = 5;
    entry.success_count = 5;
    entry.last_used = now;
    let high_usage_score = entry.compute_utility(now, 0.01);
    assert!(
        high_usage_score > initial_score,
        "Higher success usage must yield higher utility"
    );

    // After 100 hours idle, decay should reduce utility
    let later = now + Duration::hours(100);
    let decayed_score = entry.compute_utility(later, 0.01);
    assert!(
        decayed_score < high_usage_score,
        "Decay must reduce utility over time"
    );
}

#[test]
fn test_error_analyst_signatures() {
    let tmp = tempdir().unwrap();
    let distiller = SkillDistiller::new(tmp.path().join("skills"), tmp.path().join("ledger.json"));

    // Borrow checker
    let skill_bc = distiller
        .analyze_error_trace("error[E0382]: borrow of moved value: `foo`", None)
        .expect("should detect borrow error");
    assert_eq!(skill_bc.name, "mitigate_rust_borrow_checker");
    assert_eq!(skill_bc.skill_type, DistilledSkillType::ErrorMitigation);

    // Unresolved symbol
    let skill_sym = distiller
        .analyze_error_trace("error[E0425]: cannot find value `bar` in this scope", None)
        .expect("should detect unresolved symbol");
    assert_eq!(skill_sym.name, "mitigate_rust_unresolved_symbol");

    // Tool validation
    let skill_tool = distiller
        .analyze_error_trace(
            "Tool validation failed: missing field 'path'",
            Some("file_edit"),
        )
        .expect("should detect tool validation failure");
    assert_eq!(skill_tool.name, "mitigate_file_edit_args_validation");

    // Timeout
    let skill_timeout = distiller
        .analyze_error_trace("command execution timed out after 120s", None)
        .expect("should detect timeout");
    assert_eq!(skill_timeout.name, "mitigate_operation_timeout");

    // Test assertion
    let skill_test = distiller
        .analyze_error_trace(
            "test cognitive::tests::failed - panicked at assertion `left == right` failed",
            None,
        )
        .expect("should detect test failure");
    assert_eq!(skill_test.name, "mitigate_test_assertion_regression");

    // Unrecognized error returns None
    assert!(distiller
        .analyze_error_trace("generic normal output without issue", None)
        .is_none());
}

#[test]
fn test_success_analyst_generates_sop() {
    let tmp = tempdir().unwrap();
    let distiller = SkillDistiller::new(tmp.path().join("skills"), tmp.path().join("ledger.json"));

    let steps = vec![
        "Check working tree cleanliness".to_string(),
        "Run compilation smoke test".to_string(),
        "Stage and commit artifacts".to_string(),
    ];
    let tools = vec!["bash".to_string(), "git".to_string()];

    let sop = distiller
        .analyze_success_trace("Deploy Project Release", &steps, &tools)
        .expect("should produce SOP");
    assert_eq!(sop.name, "sop_deploy_project_release");
    assert_eq!(
        sop.skill_type,
        DistilledSkillType::StandardOperatingProcedure
    );
    assert_eq!(sop.tools, tools);
    assert!(sop.content.contains("1. Check working tree cleanliness"));
    assert!(sop.content.contains("2. Run compilation smoke test"));

    // Empty steps return None
    assert!(distiller
        .analyze_success_trace("Goal", &[], &tools)
        .is_none());
    // Empty goal returns None
    assert!(distiller
        .analyze_success_trace("   ", &steps, &tools)
        .is_none());
}

#[test]
fn test_metis_pattern_miner_mines_frequent_sequences() {
    let tmp = tempdir().unwrap();
    let distiller = SkillDistiller::new(tmp.path().join("skills"), tmp.path().join("ledger.json"))
        .with_min_pattern_support(2);

    let seq1 = vec![
        "file_read".to_string(),
        "file_edit".to_string(),
        "cargo_check".to_string(),
    ];
    let seq2 = vec![
        "file_edit".to_string(),
        "cargo_check".to_string(),
        "cargo_test".to_string(),
    ];
    let seq3 = vec![
        "bash".to_string(),
        "file_read".to_string(),
        "file_edit".to_string(),
    ];

    let patterns = distiller.mine_metis_patterns(&[seq1, seq2, seq3]);
    // file_read -> file_edit occurs in seq1 and seq3 (count=2)
    // file_edit -> cargo_check occurs in seq1 and seq2 (count=2)
    assert!(!patterns.is_empty());
    let names: Vec<_> = patterns.iter().map(|p| p.name.clone()).collect();
    assert!(names.contains(&"composite_file_read_and_file_edit".to_string()));
    assert!(names.contains(&"composite_file_edit_and_cargo_check".to_string()));
}

#[test]
fn test_distill_from_session_events_and_ledger_persistence() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    let ledger_file = tmp.path().join("ledger.json");
    let mut distiller = SkillDistiller::new(skills_dir.clone(), ledger_file.clone());

    let now = Utc::now();
    let events = vec![
        SessionLogEvent {
            timestamp: now,
            session_id: "s1".to_string(),
            event_type: SessionEventType::ToolCall,
            task_id: Some("t1".to_string()),
            tool_name: Some("cargo_check".to_string()),
            input: None,
            arguments: None,
            result: Some("error[E0382]: use of moved value".to_string()),
            success: Some(false),
            duration_ms: Some(500),
            details: None,
        },
        SessionLogEvent {
            timestamp: now,
            session_id: "s1".to_string(),
            event_type: SessionEventType::ToolValidationFailed,
            task_id: Some("t1".to_string()),
            tool_name: Some("file_edit".to_string()),
            input: None,
            arguments: None,
            result: Some("Tool validation failed: invalid path".to_string()),
            success: Some(false),
            duration_ms: Some(10),
            details: None,
        },
    ];

    let report = distiller
        .distill_from_session_events(&events)
        .expect("distillation should succeed");
    // Strictly exactly 2 skills: ToolValidationFailed must not be distilled twice
    assert_eq!(report.skills_created, 2);
    assert!(skills_dir.join("mitigate_rust_borrow_checker.md").exists());
    assert!(skills_dir
        .join("mitigate_file_edit_args_validation.md")
        .exists());

    // Ledger should exist and contain entries
    let ledger = distiller.load_ledger().expect("ledger should load");
    assert!(ledger.contains_key("mitigate_rust_borrow_checker"));
    assert!(ledger.contains_key("mitigate_file_edit_args_validation"));

    // Record usage
    distiller
        .record_skill_usage("mitigate_rust_borrow_checker", true)
        .expect("record usage should succeed");
    let updated_ledger = distiller.load_ledger().expect("ledger should load");
    let entry = updated_ledger.get("mitigate_rust_borrow_checker").unwrap();
    assert_eq!(entry.invocations, 1);
    assert_eq!(entry.success_count, 1);

    // Nonexistent skill returns error
    assert!(distiller.record_skill_usage("nonexistent", true).is_err());
}

#[test]
fn test_capacity_cap_enforcement_and_eviction() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    let ledger_file = tmp.path().join("ledger.json");
    // Cap at 3 skills
    let mut distiller =
        SkillDistiller::new(skills_dir.clone(), ledger_file.clone()).with_max_skills_cap(3);

    let mut candidates = Vec::new();
    for i in 1..=5 {
        candidates.push(DistilledSkill {
            name: format!("skill_{}", i),
            skill_type: DistilledSkillType::ErrorMitigation,
            description: format!("Skill {}", i),
            tools: vec![],
            triggers: vec![],
            content: format!("Skill {} content\n", i),
        });
    }

    let report = distiller
        .commit_distilled_skills(candidates)
        .expect("commit should succeed");

    assert_eq!(report.skills_created, 5);
    assert_eq!(report.skills_evicted, 2);

    let ledger = distiller.load_ledger().unwrap();
    assert_eq!(ledger.len(), 3, "Ledger must strictly respect cap = 3");

    // Check files on disk
    let remaining_files: Vec<_> = fs::read_dir(&skills_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(remaining_files.len(), 3, "Disk must only keep 3 skills");
}

#[test]
fn test_distill_from_collected_items() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    let ledger_file = tmp.path().join("ledger.json");
    let mut distiller = SkillDistiller::new(skills_dir, ledger_file);

    let now = Utc::now();
    let items = vec![
        CollectedItem {
            source_id: "item1".to_string(),
            source_type: SourceType::Episode,
            content: "Run End To End Smoke Tests\nVerify database connectivity\nAssert all smoke probes return 200".to_string(),
            timestamp: now,
            importance: 3,
            tags: vec!["bash".to_string()],
            metadata: HashMap::new(),
            related_ids: vec![],
            session_id: Some("s1".to_string()),
            file_refs: vec![],
        },
        CollectedItem {
            source_id: "item2".to_string(),
            source_type: SourceType::ToolResult,
            content: "error[E0425]: cannot find value `xyz` in scope".to_string(),
            timestamp: now,
            importance: 2,
            tags: vec![],
            metadata: HashMap::new(),
            related_ids: vec![],
            session_id: Some("s1".to_string()),
            file_refs: vec![],
        },
    ];

    let report = distiller.distill_from_collected_items(&items).unwrap();
    assert!(report.skills_created >= 2);
}

#[test]
fn test_skill_distiller_getters_and_builders() {
    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    let ledger_file = tmp.path().join("ledger.json");
    let distiller = SkillDistiller::new(skills_dir.clone(), ledger_file.clone())
        .with_max_skills_cap(10)
        .with_min_pattern_support(3);

    assert_eq!(distiller.skills_dir(), skills_dir.as_path());
    assert_eq!(distiller.ledger_file(), ledger_file.as_path());
    assert_eq!(distiller.max_skills_cap, 10);
    assert_eq!(distiller.min_pattern_support, 3);
}

#[test]
fn test_commit_distilled_skills_updates_existing_skill() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    let ledger_file = tmp.path().join("ledger.json");
    let mut distiller = SkillDistiller::new(skills_dir, ledger_file);

    let skill = DistilledSkill {
        name: "test_update".to_string(),
        skill_type: DistilledSkillType::ErrorMitigation,
        description: "Initial description".to_string(),
        tools: vec!["bash".to_string()],
        triggers: vec![],
        content: "Initial content\n".to_string(),
    };

    let report1 = distiller
        .commit_distilled_skills(vec![skill.clone()])
        .expect("initial commit");
    assert_eq!(report1.skills_created, 1);
    assert_eq!(report1.skills_updated, 0);

    // Commit updated version
    let mut updated_skill = skill;
    updated_skill.description = "Updated description".to_string();
    let report2 = distiller
        .commit_distilled_skills(vec![updated_skill])
        .expect("update commit");
    assert_eq!(report2.skills_created, 0);
    assert_eq!(report2.skills_updated, 1);
}

#[test]
fn test_consolidation_engine_with_skill_distiller() {
    let tmp = tempdir().unwrap();
    let config = ConsolidationConfig::new("http://localhost:8000/v1", "qwen38");
    let distiller = SkillDistiller::new(tmp.path().join("skills"), tmp.path().join("ledger.json"));

    let mut engine = ConsolidationEngine::new(config)
        .unwrap()
        .with_skill_distiller(distiller);

    assert!(engine.skill_distiller().is_some());
    assert!(engine.skill_distiller_mut().is_some());
}

#[test]
fn test_tool_name_sanitization_prevents_path_traversal() {
    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    let ledger_file = tmp.path().join("ledger.json");
    let mut distiller = SkillDistiller::new(skills_dir.clone(), ledger_file);

    // 1. In analyze_error_trace with path traversal in tool_hint
    let skill = distiller
        .analyze_error_trace(
            "validation failed: missing arg",
            Some("../../../etc/shadow"),
        )
        .expect("should produce skill");
    assert!(!skill.name.contains('/'));
    assert!(!skill.name.contains('.'));
    assert_eq!(skill.name, "mitigate_etcshadow_args_validation");

    // 2. In commit_distilled_skills with malicious candidate name
    let malicious = DistilledSkill {
        name: "../../evil_skill".to_string(),
        skill_type: DistilledSkillType::ErrorMitigation,
        description: "attempted traversal".to_string(),
        tools: vec![],
        triggers: vec![],
        content: "malicious content\n".to_string(),
    };
    let report = distiller
        .commit_distilled_skills(vec![malicious])
        .expect("should sanitize name safely");
    assert_eq!(report.skills_created, 1);
    assert_eq!(report.created_skill_names[0], "evil_skill");
    assert!(skills_dir.join("evil_skill.md").exists());
}

#[test]
fn test_eviction_deterministic_tie_breaking() {
    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    let ledger_file = tmp.path().join("ledger.json");
    // Capacity 2
    let mut distiller = SkillDistiller::new(skills_dir, ledger_file).with_max_skills_cap(2);

    let candidates = vec![
        DistilledSkill {
            name: "skill_c".to_string(),
            skill_type: DistilledSkillType::ErrorMitigation,
            description: "C".to_string(),
            tools: vec![],
            triggers: vec![],
            content: "C content\n".to_string(),
        },
        DistilledSkill {
            name: "skill_a".to_string(),
            skill_type: DistilledSkillType::ErrorMitigation,
            description: "A".to_string(),
            tools: vec![],
            triggers: vec![],
            content: "A content\n".to_string(),
        },
        DistilledSkill {
            name: "skill_b".to_string(),
            skill_type: DistilledSkillType::ErrorMitigation,
            description: "B".to_string(),
            tools: vec![],
            triggers: vec![],
            content: "B content\n".to_string(),
        },
    ];

    let report = distiller.commit_distilled_skills(candidates).unwrap();
    assert_eq!(report.skills_created, 3);
    assert_eq!(report.skills_evicted, 1);
    // When utility and last_used tie, lowest lexicographical name ("skill_a") is deterministically evicted first
    assert_eq!(report.evicted_skill_names, vec!["skill_a".to_string()]);
}

#[test]
fn test_load_and_save_ledger_reject_symlinks() {
    let tmp = tempdir().unwrap();
    let real_file = tmp.path().join("real_target.json");
    fs::write(&real_file, b"{}").unwrap();

    let symlinked_ledger = tmp.path().join("symlinked_ledger.json");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_file, &symlinked_ledger).unwrap();

    #[cfg(unix)]
    {
        let distiller = SkillDistiller::new(tmp.path().join("skills"), symlinked_ledger);
        let err = distiller.load_ledger().unwrap_err();
        assert!(err.to_string().contains("Ledger destination is a symlink"));

        let ledger = HashMap::new();
        let err = distiller.save_ledger(&ledger).unwrap_err();
        assert!(err.to_string().contains("Ledger destination is a symlink"));

        // Test tmp_path symlink rejection
        let direct_ledger = tmp.path().join("direct_ledger.json");
        let tmp_path = direct_ledger.with_extension("tmp");
        std::os::unix::fs::symlink(&real_file, &tmp_path).unwrap();

        let distiller_tmp = SkillDistiller::new(tmp.path().join("skills"), direct_ledger);
        let err = distiller_tmp.save_ledger(&ledger).unwrap_err();
        assert!(err
            .to_string()
            .contains("Ledger temporary destination is a symlink"));
    }
}

#[test]
fn test_commit_distilled_skills_rejects_destination_symlink() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let tmp = tempdir().unwrap();
    let skills_dir = tmp.path().join("skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let outside = tmp.path().join("outside.md");
    fs::write(&outside, b"outside content").unwrap();

    let symlink_path = skills_dir.join("sym_skill.md");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &symlink_path).unwrap();

    #[cfg(unix)]
    {
        let mut distiller = SkillDistiller::new(skills_dir, tmp.path().join("ledger.json"));
        let candidate = DistilledSkill {
            name: "sym_skill".to_string(),
            skill_type: DistilledSkillType::ErrorMitigation,
            description: "symlink test".to_string(),
            tools: vec![],
            triggers: vec![],
            content: "payload\n".to_string(),
        };

        let err = distiller
            .commit_distilled_skills(vec![candidate])
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("Destination skill path is a symlink"));
        assert_eq!(
            fs::read(&outside).unwrap(),
            b"outside content",
            "outside target must not be overwritten"
        );
    }
}

#[test]
fn test_killswitch_blocks_distillation_and_usage_recording() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let tmp = tempdir().unwrap();
    let mut distiller =
        SkillDistiller::new(tmp.path().join("skills"), tmp.path().join("ledger.json"));

    // Trip killswitch
    crate::safety::killswitch::trip_in_process("Distiller test killswitch");

    assert!(distiller.distill_from_session_events(&[]).is_err());
    assert!(distiller.distill_from_collected_items(&[]).is_err());
    assert!(distiller.record_skill_usage("test_skill", true).is_err());

    // Reset cleanly
    crate::safety::killswitch::reset_in_process();
}
