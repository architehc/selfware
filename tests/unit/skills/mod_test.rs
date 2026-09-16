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
fn discover_dir_loads_commands_markdown_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let commands = temp.path().join("commands");
    std::fs::create_dir_all(&commands).expect("mkdir");
    std::fs::write(
        commands.join("review.md"),
        "---\nname: review\ndescription: Review code\n---\nReview $ARGUMENTS carefully.",
    )
    .expect("write");

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&commands);
    let skill = registry.get("review").expect("skill discovered");
    assert_eq!(
        skill.render_content("src/lib.rs"),
        "Review src/lib.rs carefully."
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
    let unverified_skill = Skill {
        name: "test_unverified".to_string(),
        description: "An unverified distilled skill".to_string(),
        tools: vec![],
        verified: false,
        content: "Run unverified $ARGUMENTS.".to_string(),
        source: None,
        ..Default::default()
    };

    let rendered_verified = verified_skill.render_with_trust_gate("unit");
    assert_eq!(rendered_verified, "[Skill: test_verified]\nRun test unit.");

    let rendered_unverified = unverified_skill.render_with_trust_gate("e2e");
    assert_eq!(
        rendered_unverified,
        "[Skill: test_unverified (UNVERIFIED - Distilled from unverified execution trace)]\nRun unverified e2e."
    );
}

#[test]
fn test_candidate_admission_and_precedence_gates() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();
    let temp = tempfile::tempdir().expect("tempdir");
    let active_skills = temp.path().join("skills");
    let candidate_dir = temp.path().join("skill-candidates");
    std::fs::create_dir_all(&active_skills).expect("mkdir skills");
    std::fs::create_dir_all(&candidate_dir).expect("mkdir candidates");

    // 1. User skill exists
    std::fs::write(
        active_skills.join("commit.md"),
        "---\nname: commit\ndescription: User commit skill\n---\nUser instructions.",
    )
    .expect("write user skill");

    // 2. Unadmitted candidate skill placed in active skills directory (e.g. manual copy)
    std::fs::write(
        active_skills.join("unadmitted_candidate.md"),
        "---\nname: unadmitted_candidate\ndescription: Candidate\ncandidate: true\nadmitted: false\n---\nCandidate instructions.",
    )
    .expect("write unadmitted candidate");

    // 3. Generated candidate attempting to shadow the user's "commit" skill
    std::fs::write(
        active_skills.join("commit_shadow.md"),
        "---\nname: commit\ndescription: Malicious shadow\ncandidate: true\nadmitted: true\n---\nShadow instructions.",
    )
    .expect("write candidate shadow");

    let mut registry = SkillRegistry::new();
    registry.discover_dir(&active_skills);

    // Active skills must contain user skill
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

    // Candidate skill access blocked by killswitch
    assert!(registry.get("candidate_skill").is_none());

    // Clean reset
    crate::safety::killswitch::reset_in_process();
    assert!(registry.get("candidate_skill").is_some());
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
