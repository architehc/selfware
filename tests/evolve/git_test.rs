use git2::Repository;
use selfware::evolve::git::GitEngine;

use crate::committed_repository;

#[test]
fn test_branch_preview_does_not_mutate_and_confirm_creates_exact_head() {
    let (project, head) = committed_repository();
    let git = GitEngine::new(project.path());

    let preview = git
        .create_branch("evolve/grounded-review", &head, false)
        .unwrap();
    assert!(!preview.created);
    assert!(preview.blockers.is_empty());
    assert_eq!(preview.head, head);
    assert!(Repository::open(project.path())
        .unwrap()
        .find_branch("evolve/grounded-review", git2::BranchType::Local)
        .is_err());

    let created = git
        .create_branch("evolve/grounded-review", &head, true)
        .unwrap();
    assert!(created.created);
    assert!(created.blockers.is_empty());
    assert_eq!(created.head, head);

    let status = git.status().unwrap();
    assert_eq!(status.branch.as_deref(), Some("evolve/grounded-review"));
    assert_eq!(status.head, head);
    assert!(!status.dirty);
}

#[test]
fn test_branch_creation_is_blocked_by_dirty_worktree() {
    let (project, head) = committed_repository();
    std::fs::write(project.path().join("README.md"), "changed\n").unwrap();
    let git = GitEngine::new(project.path());

    let status = git.status().unwrap();
    assert!(status.dirty);
    assert_eq!(status.files.len(), 1);
    assert_eq!(status.files[0].path, "README.md");
    assert_eq!(status.files[0].index, "clean");
    assert_eq!(status.files[0].worktree, "modified");

    let result = git.create_branch("evolve/blocked", &head, true).unwrap();
    assert!(!result.created);
    assert!(result
        .blockers
        .iter()
        .any(|blocker| blocker.contains("working tree has 1 uncommitted path")));
    assert!(Repository::open(project.path())
        .unwrap()
        .find_branch("evolve/blocked", git2::BranchType::Local)
        .is_err());
}

#[test]
fn test_branch_creation_is_blocked_by_stale_head_and_existing_name() {
    let (project, head) = committed_repository();
    let repo = Repository::open(project.path()).unwrap();
    let commit = repo
        .find_commit(git2::Oid::from_str(&head).unwrap())
        .unwrap();
    repo.branch("already-there", &commit, false).unwrap();
    drop(commit);
    drop(repo);
    let git = GitEngine::new(project.path());

    let result = git
        .create_branch("already-there", &"0".repeat(40), true)
        .unwrap();

    assert!(!result.created);
    assert!(result
        .blockers
        .iter()
        .any(|blocker| blocker.contains("HEAD changed")));
    assert!(result
        .blockers
        .iter()
        .any(|blocker| blocker.contains("branch already exists")));
}

#[test]
fn test_branch_creation_rejects_invalid_names_before_mutation() {
    let (project, head) = committed_repository();
    let git = GitEngine::new(project.path());

    for name in ["", "-unsafe", "bad..name", "trailing/", "bad name"] {
        assert!(git.create_branch(name, &head, true).is_err(), "{name}");
    }
    assert_eq!(git.status().unwrap().head, head);
}

#[test]
fn test_branch_creation_respects_external_head_ref_lock() {
    let (project, head) = committed_repository();
    let locking_repo = Repository::open(project.path()).unwrap();
    let mut transaction = locking_repo.transaction().unwrap();
    transaction.lock_ref("HEAD").unwrap();

    let git = GitEngine::new(project.path());
    let error = git
        .create_branch("evolve/locked-head", &head, true)
        .unwrap_err();

    assert!(error.to_string().to_lowercase().contains("lock"));
    assert!(Repository::open(project.path())
        .unwrap()
        .find_branch("evolve/locked-head", git2::BranchType::Local)
        .is_err());
}
