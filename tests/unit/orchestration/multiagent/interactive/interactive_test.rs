use super::*;

#[test]
fn stdin_eof_is_classified_for_loop_exit() {
    // Ok(0) is EOF: the interactive loops must break on it, not continue.
    // Continuing reprinted the prompt forever — piped stdin without a
    // trailing `exit` livelocked at full CPU (the P0-1 regression).
    assert_eq!(classify_stdin_read(&Ok(0)), StdinRead::Eof);
}

#[test]
fn stdin_line_read_is_not_eof() {
    assert_eq!(classify_stdin_read(&Ok(1)), StdinRead::Line);
    assert_eq!(classify_stdin_read(&Ok(42)), StdinRead::Line);
}

#[test]
fn stdin_error_is_retriable() {
    let err = std::io::Error::new(std::io::ErrorKind::Interrupted, "interrupted");
    assert_eq!(classify_stdin_read(&Err(err)), StdinRead::Error);
}

#[test]
fn exit_words_match_case_insensitively() {
    for word in [
        "exit", "Exit", "EXIT", "quit", "Quit", "QUIT", "/exit", "/quit", "q", "Q", "/q",
    ] {
        assert!(is_exit_word(word), "expected exit word: {}", word);
    }
}

#[test]
fn non_exit_words_do_not_exit() {
    for word in ["", "exi", "exit now", "/help", "quite", "/parallel 8"] {
        assert!(!is_exit_word(word), "unexpected exit word: {}", word);
    }
}

#[test]
fn role_swarm_mirrors_chat_fleet_one_to_one() {
    let roles = [
        AgentRole::Architect,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
    ];
    let (swarm, ids) = build_role_swarm(&roles);
    assert_eq!(ids.len(), roles.len());
    for (i, id) in ids.iter().enumerate() {
        let agent = swarm.get_agent(id).expect("swarm agent exists");
        assert_eq!(agent.role, roles[i]);
        assert_eq!(agent.status, crate::swarm::AgentStatus::Idle);
    }
}

#[test]
fn parse_agent_role_recognizes_all_roles_and_rejects_unknown() {
    assert_eq!(parse_agent_role("architect"), Some(AgentRole::Architect));
    assert_eq!(parse_agent_role("  CODER "), Some(AgentRole::Coder));
    assert_eq!(parse_agent_role("tester"), Some(AgentRole::Tester));
    assert_eq!(parse_agent_role("reviewer"), Some(AgentRole::Reviewer));
    assert_eq!(parse_agent_role("documenter"), Some(AgentRole::Documenter));
    assert_eq!(parse_agent_role("devops"), Some(AgentRole::DevOps));
    assert_eq!(parse_agent_role("security"), Some(AgentRole::Security));
    assert_eq!(
        parse_agent_role("performance"),
        Some(AgentRole::Performance)
    );
    assert_eq!(
        parse_agent_role("visual-critic"),
        Some(AgentRole::VisualCritic)
    );
    assert_eq!(parse_agent_role("general"), Some(AgentRole::General));
    assert_eq!(parse_agent_role("unknown"), None);
    const {
        assert!(MAX_TOTAL_AGENTS >= MAX_CONCURRENT_AGENTS);
    };
}
