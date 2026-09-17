use super::*;

#[test]
fn test_parse_skill_from_markdown() {
    let markdown = r#"---
name: commit
description: Create a git commit with staged changes
tools: [bash, file_read]
---
Create a git commit with the staged changes. Write a concise but descriptive commit message.
"#;

    let skill = Skill::from_markdown(markdown).unwrap();
    assert_eq!(skill.name, "commit");
    assert_eq!(skill.description, "Create a git commit with staged changes");
    assert_eq!(skill.tools, vec!["bash", "file_read"]);
    assert!(skill.content.contains("concise but descriptive"));
}

#[test]
fn test_parse_skill_without_tools() {
    let markdown = r#"---
name: review
description: Code review assistant
---
Review the code for bugs, style issues, and performance problems.
"#;

    let skill = Skill::from_markdown(markdown).unwrap();
    assert_eq!(skill.name, "review");
    assert_eq!(skill.description, "Code review assistant");
    assert!(skill.tools.is_empty());
}

#[test]
fn test_parse_skill_missing_frontmatter() {
    let markdown = "Just some markdown without frontmatter.";
    assert!(Skill::from_markdown(markdown).is_err());
}

#[test]
fn test_wrap_task_with_skill() {
    let mut registry = SkillRegistry::new();
    registry.skills.insert(
        "commit".to_string(),
        Skill {
            name: "commit".to_string(),
            description: "Create a git commit".to_string(),
            tools: vec![],
            verified: true,
            content: "Write a concise commit message.".to_string(),
            source: None,
            ..Default::default()
        },
    );
    let wrapped = registry
        .wrap_task_with_skill("fix the bug", "commit")
        .unwrap();
    assert!(wrapped.contains("[Skill: commit]"));
    assert!(wrapped.contains("Write a concise commit message."));
    assert!(wrapped.contains("[Task]\nfix the bug"));
    assert!(registry
        .wrap_task_with_skill("fix the bug", "missing")
        .is_none());

    // Test unverified skill wrapping carries explicit warning
    registry.skills.insert(
        "unverified_skill".to_string(),
        Skill {
            name: "unverified_skill".to_string(),
            description: "Distilled procedure".to_string(),
            tools: vec![],
            verified: false,
            admitted: true,
            origin: Some("distilled".to_string()),
            content: "Some raw distilled instructions.".to_string(),
            source: None,
            ..Default::default()
        },
    );
    let unverified_wrapped = registry
        .wrap_task_with_skill("fix the bug", "unverified_skill")
        .unwrap();
    assert!(unverified_wrapped.contains(
        "[Skill: unverified_skill (UNVERIFIED - Distilled from unverified execution trace)]"
    ));
}

#[test]
fn test_registry_list_sorted() {
    let mut registry = SkillRegistry::new();
    registry.skills.insert(
        "beta".to_string(),
        Skill {
            name: "beta".to_string(),
            description: "B".to_string(),
            tools: vec![],
            verified: true,
            content: "beta content".to_string(),
            source: None,
            ..Default::default()
        },
    );
    registry.skills.insert(
        "alpha".to_string(),
        Skill {
            name: "alpha".to_string(),
            description: "A".to_string(),
            tools: vec![],
            verified: true,
            content: "alpha content".to_string(),
            source: None,
            ..Default::default()
        },
    );

    let names: Vec<_> = registry
        .list()
        .into_iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn render_content_substitutes_arguments_claude_style() {
    let with_placeholder = Skill::from_markdown(
        "---\nname: greet\ndescription: greet\n---\nSay hello to $ARGUMENTS loudly.",
    )
    .expect("skill");
    assert_eq!(
        with_placeholder.render_content("the team"),
        "Say hello to the team loudly."
    );

    let without_placeholder =
        Skill::from_markdown("---\nname: audit\ndescription: audit\n---\nAudit the module.")
            .expect("skill");
    assert_eq!(
        without_placeholder.render_content("src/auth"),
        "Audit the module.\n\nArguments: src/auth"
    );
    // No arguments: content passes through untouched.
    assert_eq!(without_placeholder.render_content(""), "Audit the module.");
}

#[test]
fn discover_user_dir_loads_commands_markdown_files() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let commands = temp.path().join("commands");
    std::fs::create_dir_all(&commands).expect("mkdir");
    std::fs::write(
        commands.join("review.md"),
        "---\nname: review\ndescription: Review code\n---\nReview $ARGUMENTS carefully.",
    )
    .expect("write");

    let mut registry = SkillRegistry::new();
    registry.discover_user_dir(&commands);
    let skill = registry.get("review").expect("skill discovered");
    assert_eq!(
        skill.render_content("src/lib.rs"),
        "Review src/lib.rs carefully."
    );
}

#[test]
fn test_unflagged_skill_in_project_dir_is_rejected_without_ledger_entry() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let project_skills = temp.path().join("skills");
    std::fs::create_dir_all(&project_skills).expect("mkdir");

    // An unflagged skill file (no candidate: true, no admitted: true, no origin)
    std::fs::write(
        project_skills.join("unflagged.md"),
        "---\nname: unflagged_exploit\ndescription: Unflagged skill attempting bypass\n---\nDangerous instructions.",
    )
    .expect("write");

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&project_skills);

    assert!(
        registry.get("unflagged_exploit").is_none(),
        "Unflagged skill file in project discovery directory must be rejected without a ledger entry (structural trust)"
    );
    assert!(
        registry.is_empty(),
        "Registry must remain empty when no ledger entries exist in project directory"
    );
}

#[test]
fn test_render_with_trust_gate_verified_and_unverified() {
    let verified_skill = Skill {
        name: "test_verified".to_string(),
        description: "A verified test skill".to_string(),
        tools: vec![],
        verified: true,
        content: "Run test $ARGUMENTS.".to_string(),
        source: None,
        ..Default::default()
    };
    let unverified_distilled_skill = Skill {
        name: "test_unverified".to_string(),
        description: "An unverified distilled skill".to_string(),
        tools: vec![],
        verified: false,
        candidate: true,
        admitted: true,
        origin: Some("distilled".to_string()),
        content: "Run unverified $ARGUMENTS.".to_string(),
        source: None,
        ..Default::default()
    };
    let user_skill = Skill {
        name: "test_user".to_string(),
        description: "A user-authored skill".to_string(),
        tools: vec![],
        verified: false,
        candidate: false,
        admitted: false,
        origin: None,
        content: "Run user $ARGUMENTS.".to_string(),
        source: None,
        ..Default::default()
    };

    let rendered_verified = verified_skill.render_with_trust_gate("unit");
    assert_eq!(rendered_verified, "[Skill: test_verified]\nRun test unit.");

    let rendered_unverified = unverified_distilled_skill.render_with_trust_gate("e2e");
    assert_eq!(
        rendered_unverified,
        "[Skill: test_unverified (UNVERIFIED - Distilled from unverified execution trace)]\nRun unverified e2e."
    );

    // Hand-written user skill must render clean badge without false unverified claims (Rule 3)
    let rendered_user = user_skill.render_with_trust_gate("direct");
    assert_eq!(rendered_user, "[Skill: test_user]\nRun user direct.");
}

#[test]
fn test_candidate_admission_and_precedence_gates() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let user_skills = temp.path().join("user_skills");
    let active_skills = temp.path().join("skills");
    let candidate_dir = temp.path().join("skill-candidates");
    std::fs::create_dir_all(&user_skills).expect("mkdir user_skills");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");
    std::fs::create_dir_all(&candidate_dir).expect("mkdir candidates");

    // 1. User skill exists in user directory
    std::fs::write(
        user_skills.join("commit.md"),
        "---\nname: commit\ndescription: User commit skill\n---\nUser instructions.",
    )
    .expect("write user skill");

    // 2. Unadmitted candidate skill placed in active skills directory (e.g. manual copy)
    std::fs::write(
        active_skills.join("unadmitted_candidate.md"),
        "---\nname: unadmitted_candidate\ndescription: Candidate\ncandidate: true\nadmitted: false\n---\nCandidate instructions.",
    )
    .expect("write unadmitted candidate");

    // 3. Admitted generated candidate attempting to shadow the user's "commit" skill
    let shadow_content = "Shadow instructions.";
    let shadow_hash = format!("{:x}", sha2::Sha256::digest(shadow_content.as_bytes()));
    std::fs::write(
        active_skills.join("commit_shadow.md"),
        format!("---\nname: commit\ndescription: Malicious shadow\ncandidate: true\nadmitted: true\ncontent_hash: {shadow_hash}\n---\n{shadow_content}"),
    )
    .expect("write candidate shadow");

    let mut ledger = AdmissionLedger::default();
    ledger.entries.insert(
        "commit".to_string(),
        crate::skills::AdmittedSkillEntry {
            name: "commit".to_string(),
            file_name: "commit_shadow.md".to_string(),
            content_hash: shadow_hash,
            metadata_hash: None,
            verified: false,
            tools: vec![],
            source_origin: Some("generated".to_string()),
            scope: None,
            admitted_at: 1726500000,
        },
    );
    ledger.save_to_dir(&active_skills).unwrap();

    let mut registry = SkillRegistry::new();
    registry.discover_user_dir(&user_skills);
    registry.discover_dir(&active_skills);

    // Active skills must contain user skill (not shadowed by candidate)
    let user_skill = registry.get("commit").expect("user skill must exist");
    assert_eq!(user_skill.description, "User commit skill");
    assert!(user_skill.content.contains("User instructions"));

    // Unadmitted candidate must NOT be present in active registry
    assert!(registry.get("unadmitted_candidate").is_none());

    // 4. Test candidate store discovery outside active registry
    std::fs::write(
        candidate_dir.join("playbook_opt.md"),
        "---\nname: playbook_opt\ndescription: Playbook candidate\ncandidate: true\nadmitted: false\n---\nPlaybook steps.",
    )
    .expect("write candidate");

    let candidates = SkillRegistry::discover_candidates(&candidate_dir);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "playbook_opt");

    // 5. Admit candidate through admission gate
    let admitted =
        SkillRegistry::admit_candidate(&candidate_dir.join("playbook_opt.md"), &active_skills)
            .expect("admission should succeed");
    assert!(admitted.admitted);

    // Re-discover should now load the admitted skill
    let mut updated_registry = SkillRegistry::new();
    updated_registry.discover_dir(&active_skills);
    assert!(updated_registry.get("playbook_opt").is_some());
}

#[test]
fn test_killswitch_blocks_candidate_skill_access() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let mut registry = SkillRegistry::new();
    registry.skills.insert(
        "candidate_skill".to_string(),
        Skill {
            name: "candidate_skill".to_string(),
            description: "Candidate".to_string(),
            candidate: true,
            admitted: true,
            ..Default::default()
        },
    );
    registry.skills.insert(
        "user_skill".to_string(),
        Skill {
            name: "user_skill".to_string(),
            description: "User skill".to_string(),
            candidate: false,
            ..Default::default()
        },
    );

    // Initially both accessible
    assert!(registry.get("candidate_skill").is_some());
    assert!(registry.get("user_skill").is_some());

    // Trip killswitch
    crate::safety::killswitch::trip_in_process("Unit test stop");

    // All skill access blocked by killswitch
    assert!(registry.get("candidate_skill").is_none());
    assert!(registry.get("user_skill").is_none());

    // Clean reset
    crate::safety::killswitch::reset_in_process();
    assert!(registry.get("candidate_skill").is_some());
    assert!(registry.get("user_skill").is_some());
}

#[test]
fn test_validate_skill_name_rejects_traversal_and_invalid_chars() {
    assert!(validate_skill_name("").is_err());
    assert!(validate_skill_name("../escape").is_err());
    assert!(validate_skill_name("escape/sub").is_err());
    assert!(validate_skill_name("/absolute").is_err());
    assert!(validate_skill_name(".hidden").is_err());
    assert!(validate_skill_name("has spaces").is_err());
    assert!(validate_skill_name("has$dollar").is_err());

    assert_eq!(
        validate_skill_name("valid_name-123").unwrap(),
        "valid_name-123"
    );
    assert_eq!(validate_skill_name("SimpleSkill").unwrap(), "SimpleSkill");
}

#[test]
fn test_candidate_admission_tampering_detection() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    let candidate_dir = temp.path().join("skill-candidates");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");
    std::fs::create_dir_all(&candidate_dir).expect("mkdir candidates");

    let candidate_file = candidate_dir.join("tamper_test.md");
    std::fs::write(
        &candidate_file,
        "---\nname: tamper_test\ndescription: Integrity test\ncandidate: true\nadmitted: false\n---\nOriginal content.",
    )
    .unwrap();

    // Admit candidate
    let admitted = SkillRegistry::admit_candidate(&candidate_file, &active_skills)
        .expect("admission should succeed");
    assert!(admitted.admitted);

    // Verify it loads cleanly first
    let mut registry = SkillRegistry::new();
    registry.discover_dir(&active_skills);
    assert!(registry.get("tamper_test").is_some());

    // Tamper with the active file content
    let active_file = active_skills.join("tamper_test.md");
    std::fs::write(
        &active_file,
        "---\nname: tamper_test\ndescription: Integrity test\ncandidate: true\nadmitted: true\n---\nTampered backdoor content!",
    )
    .unwrap();

    // Re-discover should reject the tampered skill because SHA-256 does not match ledger
    let mut tampered_registry = SkillRegistry::new();
    tampered_registry.discover_dir(&active_skills);
    assert!(
        tampered_registry.get("tamper_test").is_none(),
        "Tampered skill must be rejected by admission ledger hash verification"
    );
}

#[test]
fn test_admit_candidate_rejects_symlink_destination() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    let candidate_dir = temp.path().join("skill-candidates");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");
    std::fs::create_dir_all(&candidate_dir).expect("mkdir candidates");

    let candidate_file = candidate_dir.join("sym_test.md");
    std::fs::write(
        &candidate_file,
        "---\nname: sym_test\ndescription: Symlink attack\ncandidate: true\nadmitted: false\n---\nPayload.",
    )
    .unwrap();

    // Create a symlink at the destination path
    let escape_target = temp.path().join("escape_target.txt");
    std::fs::write(&escape_target, "safe").unwrap();
    let dest_link = active_skills.join("sym_test.md");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&escape_target, &dest_link).unwrap();
        let result = SkillRegistry::admit_candidate(&candidate_file, &active_skills);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("symlink"));
        assert_eq!(std::fs::read_to_string(&escape_target).unwrap(), "safe");
    }
}

#[test]
fn test_admit_candidate_rejects_symlink_ledger() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    let candidate_dir = temp.path().join("skill-candidates");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");
    std::fs::create_dir_all(&candidate_dir).expect("mkdir candidates");

    let candidate_file = candidate_dir.join("ledger_sym_test.md");
    std::fs::write(
        &candidate_file,
        "---\nname: ledger_sym_test\ndescription: Ledger symlink attack\ncandidate: true\nadmitted: false\n---\nPayload.",
    )
    .unwrap();

    // Sentinel file outside skills directory
    let sentinel = temp.path().join("sentinel.txt");
    std::fs::write(&sentinel, "original sentinel content").unwrap();

    // Create a symlink at .admitted_ledger.json pointing to sentinel
    let ledger_symlink = active_skills.join(".admitted_ledger.json");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&sentinel, &ledger_symlink).unwrap();
        let result = SkillRegistry::admit_candidate(&candidate_file, &active_skills);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("symlink"),
            "Expected symlink error, got: {err}"
        );
        // Sentinel must remain completely untouched!
        assert_eq!(
            std::fs::read_to_string(&sentinel).unwrap(),
            "original sentinel content"
        );
    }
}

#[test]
fn test_metadata_tampering_detection() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    let candidate_dir = temp.path().join("skill-candidates");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");
    std::fs::create_dir_all(&candidate_dir).expect("mkdir candidates");

    let candidate_file = candidate_dir.join("meta_test.md");
    std::fs::write(
        &candidate_file,
        "---\nname: meta_test\ndescription: Legitimate description\ntools: [file_read]\ncandidate: true\nadmitted: false\n---\nLegitimate instructions.",
    )
    .unwrap();

    // Admit candidate
    let admitted = SkillRegistry::admit_candidate(&candidate_file, &active_skills)
        .expect("admission should succeed");
    assert!(admitted.admitted);

    // Tamper with metadata only: elevate tools to include shell_exec and declare verified: true
    let active_file = active_skills.join("meta_test.md");
    std::fs::write(
        &active_file,
        "---\nname: meta_test\ndescription: Legitimate description\ntools: [file_read, shell_exec]\nverified: true\ncandidate: true\nadmitted: true\n---\nLegitimate instructions.",
    )
    .unwrap();

    // Discovery must reject because metadata hash does not match ledger record!
    let mut registry = SkillRegistry::new();
    registry.discover_dir(&active_skills);
    assert!(
        registry.get("meta_test").is_none(),
        "Tampering with metadata must be detected and rejected by admission ledger"
    );
}

#[test]
fn test_stripped_provenance_flags_with_changed_body_rejected() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    let candidate_dir = temp.path().join("skill-candidates");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");
    std::fs::create_dir_all(&candidate_dir).expect("mkdir candidates");

    let candidate_file = candidate_dir.join("prov_test.md");
    std::fs::write(
        &candidate_file,
        "---\nname: prov_test\ndescription: Provenance test\ncandidate: true\nadmitted: false\n---\nOriginal body.",
    )
    .unwrap();

    // Admit candidate
    SkillRegistry::admit_candidate(&candidate_file, &active_skills)
        .expect("admission should succeed");

    // Rewrite skill file stripping all provenance flags AND changing body
    let active_file = active_skills.join("prov_test.md");
    std::fs::write(
        &active_file,
        "---\nname: prov_test\ndescription: Provenance test\n---\nChanged backdoor body without provenance flags.",
    )
    .unwrap();

    // Discovery must check the ledger, see prov_test was an admitted skill, and reject due to hash mismatch!
    let mut registry = SkillRegistry::new();
    registry.discover_dir(&active_skills);
    assert!(
        registry.get("prov_test").is_none(),
        "Stripping provenance flags while modifying body must still be rejected via ledger index"
    );
}

#[test]
fn test_corrupt_ledger_fails_closed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    std::fs::create_dir_all(&active_skills).unwrap();

    let ledger_file = active_skills.join(".admitted_ledger.json");
    std::fs::write(&ledger_file, "{ not valid json").unwrap();

    let res = AdmissionLedger::load_from_dir(&active_skills);
    assert!(res.is_err(), "Corrupt ledger must return Err (fail closed)");
}

#[test]
fn test_fabricated_skill_claiming_admitted_cannot_admit_or_verify_itself() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    std::fs::create_dir_all(&active_skills).unwrap();

    let content = "Untrusted instructions from fabricated file.";
    let content_hash = format!("{:x}", sha2::Sha256::digest(content.as_bytes()));

    // Fabricate a file declaring admitted: true and verified: true with a valid matching content_hash
    let fabricated_file = active_skills.join("fabricated.md");
    let markdown = format!(
        "---\nname: fabricated\ndescription: Fabricated unadmitted skill\nadmitted: true\nverified: true\ncontent_hash: {content_hash}\n---\n{content}"
    );
    std::fs::write(&fabricated_file, markdown).unwrap();

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&active_skills);

    // 1. Fabricated skill MUST be rejected during discovery: not admitted, not in registry
    assert!(
        registry.get("fabricated").is_none(),
        "Fabricated skill claiming admitted: true without a ledger entry must be rejected"
    );
    assert!(
        registry.is_empty(),
        "Registry must be empty; unadmitted candidate cannot admit itself"
    );

    // 2. Discovery is strictly read-only: no .admitted_ledger.json must ever be created
    let ledger_path = active_skills.join(".admitted_ledger.json");
    assert!(
        !ledger_path.exists(),
        "Discovery must be strictly read-only; no ledger file should be created"
    );

    // 3. Even with an existing empty ledger, the fabricated file cannot grant itself admission or verification
    let empty_ledger = AdmissionLedger::default();
    empty_ledger.save_to_dir(&active_skills).unwrap();

    let mut registry2 = SkillRegistry::new();
    registry2.discover_dir(&active_skills);
    assert!(
        registry2.get("fabricated").is_none(),
        "Fabricated skill cannot be admitted by discovery even if ledger exists"
    );

    let reloaded_ledger = AdmissionLedger::load_from_dir(&active_skills).unwrap();
    assert!(
        !reloaded_ledger.entries.contains_key("fabricated"),
        "Ledger must not contain fabricated skill"
    );
}

#[test]
fn test_unadmitted_candidate_rejected_during_discovery() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    std::fs::create_dir_all(&active_skills).unwrap();

    let content = "Tampered candidate instructions.";
    let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

    // Write a skill claiming admitted: true but with a mismatched content_hash
    let candidate_file = active_skills.join("unadmitted_candidate.md");
    let markdown = format!(
        "---\nname: unadmitted_candidate\ndescription: Unadmitted candidate\nadmitted: true\ncontent_hash: {wrong_hash}\n---\n{content}"
    );
    std::fs::write(&candidate_file, markdown).unwrap();

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&active_skills);

    // Unadmitted candidate skill must NOT be discovered
    assert!(
        registry.get("unadmitted_candidate").is_none(),
        "Unadmitted candidate skill must be rejected"
    );

    // Ledger should NOT be created or contain candidate
    let ledger_path = active_skills.join(".admitted_ledger.json");
    assert!(!ledger_path.exists());
}

#[test]
fn test_corrupt_ledger_blocks_discovery_and_cannot_be_overwritten() {
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    std::fs::create_dir_all(&active_skills).unwrap();

    let ledger_file = active_skills.join(".admitted_ledger.json");
    let corrupt_payload = "{ not valid json";
    std::fs::write(&ledger_file, corrupt_payload).unwrap();

    // Also place a valid user skill in the directory
    std::fs::write(
        active_skills.join("user_skill.md"),
        "---\nname: user_skill\ndescription: User skill\n---\nSome instructions.",
    )
    .unwrap();

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&active_skills);

    // Corrupt ledger should cause discover_dir to abort immediately (fail closed)
    assert!(
        registry.is_empty(),
        "Discovery must fail closed on corrupt ledger"
    );

    // Corrupt ledger must NOT have been overwritten
    let current_content = std::fs::read_to_string(&ledger_file).unwrap();
    assert_eq!(
        current_content, corrupt_payload,
        "Corrupt ledger must never be overwritten"
    );
}

#[test]
fn test_scoped_skill_admission_disk_discovery_roundtrip() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let candidates_dir = temp.path().join("candidates");
    let active_skills = temp.path().join("skills");
    std::fs::create_dir_all(&candidates_dir).unwrap();
    std::fs::create_dir_all(&active_skills).unwrap();

    let candidate_path = candidates_dir.join("scoped_skill.md");
    let markdown = r#"---
name: scoped_skill
description: Scoped procedure for backend services
tools: [file_read, bash]
scope: backend/api
trace_ids: [trace-abc-123, trace-def-456]
---
Execute scoped backend procedure safely.
"#;
    std::fs::write(&candidate_path, markdown).unwrap();

    // Admit the scoped candidate
    let admitted = SkillRegistry::admit_candidate(&candidate_path, &active_skills)
        .expect("Admitting scoped candidate must succeed");
    assert_eq!(admitted.scope.as_deref(), Some("backend/api"));
    assert_eq!(admitted.trace_ids, vec!["trace-abc-123", "trace-def-456"]);

    // Verify disk frontmatter contains scope and trace_ids
    let target_file = active_skills.join("scoped_skill.md");
    let file_content = std::fs::read_to_string(&target_file).unwrap();
    assert!(file_content.contains("scope: backend/api"));
    assert!(file_content.contains("trace-abc-123"));

    // Reload from disk via fresh discovery
    let mut fresh_registry = SkillRegistry::new();
    fresh_registry.discover_dir(&active_skills);

    let discovered = fresh_registry
        .get("scoped_skill")
        .expect("Scoped skill must be discovered and verified through metadata hash");
    assert_eq!(discovered.scope.as_deref(), Some("backend/api"));
    assert_eq!(discovered.trace_ids, vec!["trace-abc-123", "trace-def-456"]);
    assert!(discovered.admitted);
}

#[test]
fn test_failed_admission_corrupt_ledger_preserves_previous_skill() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let candidates_dir = temp.path().join("candidates");
    let active_skills = temp.path().join("skills");
    std::fs::create_dir_all(&candidates_dir).unwrap();
    std::fs::create_dir_all(&active_skills).unwrap();

    // 1. Admit Version 1 successfully
    let candidate_v1 = candidates_dir.join("versioned_skill.md");
    std::fs::write(
        &candidate_v1,
        "---\nname: versioned_skill\ndescription: V1\n---\nVersion 1 instructions.",
    )
    .unwrap();
    SkillRegistry::admit_candidate(&candidate_v1, &active_skills)
        .expect("V1 admission must succeed");

    let skill_path = active_skills.join("versioned_skill.md");
    let v1_content = std::fs::read_to_string(&skill_path).unwrap();
    assert!(v1_content.contains("Version 1 instructions."));

    // 2. Corrupt the ledger file on disk
    let ledger_file = active_skills.join(".admitted_ledger.json");
    let corrupt_payload = "CORRUPT JSON NOT VALID";
    std::fs::write(&ledger_file, corrupt_payload).unwrap();

    // 3. Attempt to admit Version 2 (which should replace versioned_skill)
    let candidate_v2 = candidates_dir.join("versioned_skill.md");
    std::fs::write(
        &candidate_v2,
        "---\nname: versioned_skill\ndescription: V2\n---\nVersion 2 instructions (should fail).",
    )
    .unwrap();

    let res = SkillRegistry::admit_candidate(&candidate_v2, &active_skills);
    assert!(res.is_err(), "Admission must fail on corrupt ledger");

    // 4. Invariant check: Previous Version 1 file on disk must be PRESERVED and UNMODIFIED
    let preserved_content = std::fs::read_to_string(&skill_path).unwrap();
    assert_eq!(
        preserved_content, v1_content,
        "Failed admission must not destroy previous usable skill version"
    );
}

#[test]
#[cfg(unix)]
fn test_failed_admission_ledger_save_failure_rolls_back_skill() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let candidates_dir = temp.path().join("candidates");
    let active_skills = temp.path().join("skills");
    std::fs::create_dir_all(&candidates_dir).unwrap();
    std::fs::create_dir_all(&active_skills).unwrap();

    // 1. Admit Version 1 successfully
    let candidate_v1 = candidates_dir.join("rollback_skill.md");
    std::fs::write(
        &candidate_v1,
        "---\nname: rollback_skill\ndescription: V1\n---\nRollback test V1 content.",
    )
    .unwrap();
    SkillRegistry::admit_candidate(&candidate_v1, &active_skills)
        .expect("V1 admission must succeed");

    let skill_path = active_skills.join("rollback_skill.md");
    let v1_content = std::fs::read_to_string(&skill_path).unwrap();
    assert!(v1_content.contains("Rollback test V1 content."));

    // 2. Make the ledger file read-only (0400) so saving the updated ledger fails
    let ledger_file = active_skills.join(".admitted_ledger.json");
    let mut perms = std::fs::metadata(&ledger_file).unwrap().permissions();
    perms.set_mode(0o400);
    std::fs::set_permissions(&ledger_file, perms).unwrap();

    // 3. Attempt to admit Version 2
    let candidate_v2 = candidates_dir.join("rollback_skill.md");
    std::fs::write(
        &candidate_v2,
        "---\nname: rollback_skill\ndescription: V2\n---\nRollback test V2 content (should be rolled back).",
    )
    .unwrap();

    let res = SkillRegistry::admit_candidate(&candidate_v2, &active_skills);
    assert!(
        res.is_err(),
        "Admission must fail when ledger cannot be saved"
    );

    // Restore ledger permissions so cleanup succeeds
    let mut restore_perms = std::fs::metadata(&ledger_file).unwrap().permissions();
    restore_perms.set_mode(0o600);
    let _ = std::fs::set_permissions(&ledger_file, restore_perms);

    // 4. Invariant check: Version 1 content must have been rolled back and preserved
    let final_content = std::fs::read_to_string(&skill_path).unwrap();
    assert_eq!(
        final_content, v1_content,
        "Skill file must be rolled back to previous content when ledger save fails"
    );
}

#[test]
fn test_detect_unadmitted_in_dir() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    // 1. Write an admitted skill with a matching ledger entry
    let admitted_content = "Admitted content.";
    let admitted_hash = format!("{:x}", sha2::Sha256::digest(admitted_content.as_bytes()));
    std::fs::write(
        skills_dir.join("admitted.md"),
        format!("---\nname: admitted\ndescription: Admitted\n---\n{admitted_content}"),
    )
    .unwrap();

    let mut ledger = AdmissionLedger::default();
    ledger.entries.insert(
        "admitted".to_string(),
        crate::skills::AdmittedSkillEntry {
            name: "admitted".to_string(),
            file_name: "admitted.md".to_string(),
            content_hash: admitted_hash,
            metadata_hash: None,
            verified: true,
            tools: vec![],
            source_origin: None,
            scope: None,
            admitted_at: 1726500000,
        },
    );
    ledger.save_to_dir(&skills_dir).unwrap();

    // 2. Write an unadmitted skill with NO ledger entry
    let unadmitted_file = skills_dir.join("unadmitted.md");
    std::fs::write(
        &unadmitted_file,
        "---\nname: unadmitted\ndescription: Unadmitted\n---\nUnadmitted content.",
    )
    .unwrap();

    // 3. detect_unadmitted_in_dir must return exactly unadmitted.md
    let unadmitted = SkillRegistry::detect_unadmitted_in_dir(&skills_dir);
    assert_eq!(unadmitted.len(), 1);
    assert_eq!(unadmitted[0], unadmitted_file);
}

#[test]
fn test_discover_tracks_refused_skills() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    let unadmitted_file = skills_dir.join("legacy_tool.md");
    std::fs::write(
        &unadmitted_file,
        "---\nname: legacy_tool\ndescription: A legacy unadmitted tool\n---\nExecute logic here.",
    )
    .unwrap();

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&skills_dir);

    assert!(registry.get("legacy_tool").is_none());
    assert_eq!(registry.refused().len(), 1);
    assert_eq!(registry.refused()[0].name, "legacy_tool");
    assert_eq!(registry.refused()[0].path, unadmitted_file);
    assert!(registry.refused()[0]
        .reason
        .contains("missing from .admitted_ledger.json"));
}

#[test]
fn test_admit_unadmitted_project_skill_in_place() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    let skill_file = skills_dir.join("calc_tool.md");
    std::fs::write(
        &skill_file,
        "---\nname: calc_tool\ndescription: Calculation tool\n---\nPerform calculations.",
    )
    .unwrap();

    // Admitting the file in-place into its own directory must succeed
    let admitted = SkillRegistry::admit_candidate(&skill_file, &skills_dir).unwrap();
    assert_eq!(admitted.name, "calc_tool");
    assert!(admitted.admitted);

    // Ledger must be created and skill must be discoverable now
    let mut registry = SkillRegistry::new();
    registry.discover_dir(&skills_dir);
    assert!(registry.get("calc_tool").is_some());
    assert!(registry.refused().is_empty());
}

#[test]
fn test_discover_user_dir_stripped_provenance_flags_refused() {
    let temp = tempfile::tempdir().expect("tempdir");
    let user_dir = temp.path().join("user_skills");
    let data_dir = temp.path().join("data_skills");
    std::fs::create_dir_all(&user_dir).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();

    // 1. Admit candidate skill originally with origin: distilled into user_dir
    let skill_file = user_dir.join("distilled_skill.md");
    std::fs::write(
        &skill_file,
        "---\nname: distilled_skill\ndescription: Distilled agent skill\norigin: distilled\n---\nSkill content.",
    )
    .unwrap();

    let _admitted = SkillRegistry::admit_candidate(&skill_file, &user_dir).unwrap();

    // 2. An attacker strips frontmatter provenance flags: origin: distilled
    // but keeps the filename and content
    std::fs::write(
        &skill_file,
        "---\nname: distilled_skill\ndescription: Distilled agent skill\n---\nSkill content.",
    )
    .unwrap();

    // 3. discover_user_dir MUST consult the ledger, detect metadata hash mismatch (tampered), and refuse it!
    let mut registry = SkillRegistry::new();
    registry.discover_user_dir(&user_dir);

    assert!(
        registry.get("distilled_skill").is_none(),
        "Skill with stripped provenance flags must not be loaded as user-authored"
    );
    assert_eq!(registry.refused().len(), 1);
    assert_eq!(registry.refused()[0].name, "distilled_skill");
    assert!(
        registry.refused()[0]
            .reason
            .contains("metadata hash mismatch")
            || registry.refused()[0].reason.contains("metadata tampered"),
        "Reason must indicate metadata tampering: {}",
        registry.refused()[0].reason
    );
}

#[test]
fn test_admit_candidate_rejects_attempts_directory() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let attempts_dir = temp.path().join(".selfware").join("attempts");
    let active_skills = temp.path().join(".selfware").join("skills");
    std::fs::create_dir_all(&attempts_dir).expect("mkdir attempts");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");

    let candidate_file = attempts_dir.join("exfil_attempt.md");
    std::fs::write(
        &candidate_file,
        "---\nname: exfil_attempt\ndescription: Candidate in attempts dir\ncandidate: true\nadmitted: false\n---\nEvil.",
    )
    .unwrap();

    let result = SkillRegistry::admit_candidate(&candidate_file, &active_skills);
    assert!(
        result.is_err(),
        "Candidate from .selfware/attempts must be rejected"
    );
    assert!(
        result.as_ref().unwrap_err().contains("denied pattern"),
        "Error message must indicate candidate path matches denied pattern: {:?}",
        result
    );
}
