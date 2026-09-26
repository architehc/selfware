//! Guards for model-supplied strings that become child-process argv.
//!
//! Most CLIs we spawn (cargo, rg, docker, npm, pip, yarn, git) parse any
//! argument that starts with `-` as an option, wherever it sits, unless a
//! `--` terminator precedes it. A model-supplied "name" such as
//! `--config=...runner=sh`, `--pre=sh`, `--privileged` or `--prefix /x`
//! therefore turns into a flag that can run arbitrary programs, widen a
//! container's privileges, or write outside the workspace. Yolo and Daemon
//! run these tools without confirmation, so the tool itself must refuse
//! option-shaped operands before anything is spawned (review, 0.9.2).
//!
//! Prefer structural fixes where the CLI supports them (`rg -e <pattern>`,
//! `git ... -- <path>`); use these guards for positional operands whose CLI
//! has no reliable terminator, and as defence in depth.

use anyhow::Result;

/// True when `value` would be parsed as an option by a typical CLI.
///
/// Leading whitespace is ignored so that `" --x"` (which some wrappers trim)
/// is treated the same as `"--x"`.
pub fn is_flag_like(value: &str) -> bool {
    value.trim_start().starts_with('-')
}

/// Refuse a model-supplied operand that the child CLI would parse as an option.
///
/// `tool` and `field` name the offending argument in the error so the model
/// can correct its call. `None` (argument absent) is always accepted.
pub fn reject_flag_like_operand(tool: &str, field: &str, value: Option<&str>) -> Result<()> {
    if let Some(v) = value {
        if is_flag_like(v) {
            anyhow::bail!(
                "{tool} `{field}` must be a name, not an option: {v:?} starts with '-' \
                 and would be parsed by the command as a flag"
            );
        }
    }
    Ok(())
}

/// [`reject_flag_like_operand`] for every item of a list operand.
pub fn reject_flag_like_operands<'a, I>(tool: &str, field: &str, values: I) -> Result<()>
where
    I: IntoIterator<Item = &'a str>,
{
    for v in values {
        reject_flag_like_operand(tool, field, Some(v))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_shaped_values_are_refused() {
        for v in ["-x", "--pre=sh", "  --privileged", "-", "--"] {
            let err = reject_flag_like_operand("t", "f", Some(v)).unwrap_err();
            assert!(
                err.to_string().contains("must be a name, not an option"),
                "{v}: {err}"
            );
        }
    }

    #[test]
    fn plain_values_are_accepted() {
        for v in ["hexyl", "a-b", "ubuntu:22.04", "", "src/x-y.rs", "pkg==1.0"] {
            assert!(reject_flag_like_operand("t", "f", Some(v)).is_ok(), "{v}");
        }
        assert!(reject_flag_like_operand("t", "f", None).is_ok());
    }

    #[test]
    fn list_guard_refuses_any_option_item() {
        assert!(reject_flag_like_operands("t", "f", ["a", "b"]).is_ok());
        assert!(reject_flag_like_operands("t", "f", ["a", "--global"]).is_err());
    }
}
