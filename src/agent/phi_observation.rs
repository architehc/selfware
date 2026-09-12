//! Shadow-mode ledger observation, wired to tool dispatch.
//!
//! One function, called from every path that executes a tool. Tools run from
//! two places — `execute_parallel_tools` and `execute_single_tool_in_batch` —
//! and a previous fix to a two-caller bug class patched one caller and shipped.
//! `phi_observation_tests` asserts both PostToolUse sites call this, so a third
//! dispatch path cannot be added without either wiring it or failing tests.
//!
//! Strictly observational. It records; it returns nothing anyone branches on.

use super::Agent;
use crate::phi::ledger::RunSnapshot;
use crate::phi::observer::{apply, classify, unattributed_count, ToolCallRecord};

impl Agent {
    /// Ledger position before a batch of tools runs.
    ///
    /// Taken per batch, not per tool: tools in a parallel batch have no
    /// meaningful order relative to each other, so a verification run in the
    /// same batch as an edit must be treated as having started before that
    /// edit. That is the conservative reading, and the ledger will decline to
    /// discharge on it.
    pub(super) fn ledger_batch_snapshot(&self) -> RunSnapshot {
        self.evidence_ledger.snapshot()
    }

    /// Record one executed tool call against the evidence ledger.
    ///
    /// `args_str` is the raw argument text as dispatched; unparseable arguments
    /// are treated as an unattributable mutation rather than as no mutation.
    pub(super) fn observe_tool_call(
        &mut self,
        name: &str,
        args_str: &str,
        success: bool,
        snapshot: RunSnapshot,
    ) {
        let parsed: serde_json::Value =
            serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
        let turn_index = self.loop_control.current_iteration();
        let record = ToolCallRecord {
            tool: name,
            arguments: &parsed,
            turn_index,
            succeeded: success,
            output: None,
        };
        let events = classify(&record);
        if events.is_empty() {
            return;
        }
        let unattributed = unattributed_count(&events);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        apply(&mut self.evidence_ledger, &events, snapshot, now_ms);

        if unattributed > 0 {
            // Not an error: an honest admission that the ledger's picture of the
            // tree is incomplete, which makes any debt figure a floor.
            tracing::debug!(
                tool = name,
                unattributed,
                "phi ledger: mutation could not be attributed to a path"
            );
        }
        tracing::debug!(
            tool = name,
            outstanding = self.evidence_ledger.outstanding().len(),
            unreviewed_lines = self
                .evidence_ledger
                .outstanding_lines(crate::phi::ledger::ObligationKind::UnreviewedChange),
            untested_lines = self
                .evidence_ledger
                .outstanding_lines(crate::phi::ledger::ObligationKind::UntestedLogic),
            "phi ledger: recorded"
        );
    }

    /// Outstanding obligations as citation lines, for turn artifacts.
    pub(crate) fn ledger_citations(&self) -> Vec<String> {
        self.evidence_ledger.citations()
    }
}

#[cfg(test)]
mod phi_observation_tests {
    //! Source-level sweep: both dispatch paths must observe.
    //!
    //! A previous two-caller bug was "fixed" by patching one caller, so this
    //! checks the wiring itself rather than trusting that it was done. It reads
    //! the dispatch source, which is crude but catches the exact regression:
    //! adding a third execution path, or deleting one of these calls.

    const DISPATCH: &str = include_str!("tool_dispatch/mod.rs");

    #[test]
    fn every_post_tool_hook_site_also_observes() {
        let hook_sites = DISPATCH.matches("HookContext::post_tool(").count();
        let observations = DISPATCH.matches("self.observe_tool_call(").count();
        assert!(
            hook_sites > 1,
            "sanity: there should be more than one dispatch path"
        );
        assert_eq!(
            observations, hook_sites,
            "every path that executes a tool must record it: {hook_sites} PostToolUse \
             site(s) but {observations} ledger observation(s). A path that runs tools \
             without observing reports zero debt for real mutations."
        );
    }

    #[test]
    fn the_snapshot_is_taken_before_tools_run_not_after() {
        // Order matters: a snapshot taken after execution would let a test
        // result claim to cover edits that landed while it ran.
        for (name, body) in [
            ("execute_parallel_tools", "async fn execute_parallel_tools"),
            (
                "execute_single_tool_in_batch",
                "async fn execute_single_tool_in_batch",
            ),
        ] {
            let start = DISPATCH.find(body).unwrap_or_else(|| {
                panic!("{name} not found; the sweep test needs updating");
            });
            let rest = &DISPATCH[start..];
            let snapshot = rest
                .find("self.ledger_batch_snapshot()")
                .unwrap_or_else(|| panic!("{name} never takes a ledger snapshot"));
            let observe = rest
                .find("self.observe_tool_call(")
                .unwrap_or_else(|| panic!("{name} never observes"));
            assert!(
                snapshot < observe,
                "{name} must snapshot before it observes"
            );
        }
    }
}
