use rootcause::{Result, prelude::*};

const DISPATCH_RUN_AGENT_PREFIX: &str = "dispatch-run-";

pub(crate) fn dispatch_run_agent_id(run_id: i64) -> String {
    debug_assert!(run_id > 0, "Dispatch run ids must be positive");
    format!("{DISPATCH_RUN_AGENT_PREFIX}{run_id}")
}

pub(crate) fn parse_dispatch_run_agent_id(agent_id: &str) -> Option<i64> {
    let id = agent_id.strip_prefix(DISPATCH_RUN_AGENT_PREFIX)?;
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let run_id = id.parse::<i64>().ok()?;
    (run_id > 0).then_some(run_id)
}

pub(crate) fn validate_agent_id(agent_id: &str) -> Result<()> {
    if agent_id.trim().is_empty() {
        bail!("agent id cannot be empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn dispatch_run_agent_ids_round_trip_valid_positive_run_ids() {
        let agent_id = dispatch_run_agent_id(42);

        assert_that!(&(agent_id)).is_equal_to("dispatch-run-42");
        assert_that!(&(parse_dispatch_run_agent_id(&agent_id))).is_equal_to(Some(42));
    }

    #[test]
    fn dispatch_run_agent_id_parser_rejects_non_canonical_ids() {
        for agent_id in [
            "",
            "codex",
            "dispatch-run-",
            "dispatch-run-0",
            "dispatch-run-+60",
            "dispatch-run- 60",
            "dispatch-run-abc",
        ] {
            assert_that!(&(parse_dispatch_run_agent_id(agent_id)))
                .with_detail_message(agent_id.to_string())
                .is_equal_to(None);
        }
    }

    #[test]
    fn agent_id_validation_rejects_blank_ids() {
        assert_that!(&(validate_agent_id("agent-a").is_ok())).is_true();
        assert_that!(&(validate_agent_id(" ").is_err())).is_true();
    }
}
