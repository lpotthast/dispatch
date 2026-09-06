use std::env;

use clap::Args;
use dispatch_api_client::{DispatchClient, ProjectClient};
use rootcause::{Result, option_ext::OptionExt, prelude::*};

const DEFAULT_API_URL: &str = "http://127.0.0.1:4000";

#[derive(Args, Debug, Default)]
pub(crate) struct ContextOverrides {
    /// Override the Dispatch API URL.
    #[arg(long)]
    api_url: Option<String>,

    /// Override the project context.
    #[arg(long)]
    project: Option<String>,

    /// Override the agent id.
    #[arg(long = "agent")]
    agent_id: Option<String>,

    /// Override the Dispatch agent-run id used for request attribution.
    #[arg(long = "agent-run")]
    agent_run_id: Option<i64>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedContext {
    client: DispatchClient,
    project: Option<String>,
    agent_id: Option<String>,
    claimed_item_id: Option<i64>,
}

impl ResolvedContext {
    pub(crate) fn client(&self) -> &DispatchClient {
        &self.client
    }

    pub(crate) fn project_client(&self) -> Result<ProjectClient<'_>> {
        Ok(self.client.project(self.project()?))
    }

    pub(crate) fn project(&self) -> Result<&str> {
        Ok(self
            .project
            .as_deref()
            .context("missing Dispatch project; pass --project or set DISPATCH_PROJECT")?)
    }

    pub(crate) fn agent_id(&self) -> Result<&str> {
        Ok(self
            .agent_id
            .as_deref()
            .context("missing Dispatch agent id; pass --agent or set DISPATCH_AGENT_ID")?)
    }

    pub(crate) fn item_id(&self, explicit: Option<i64>) -> Result<i64> {
        Ok(explicit
            .or(self.claimed_item_id)
            .context("missing item id; pass an item id or set DISPATCH_CLAIMED_ITEM_ID")?)
    }
}

pub(crate) fn resolve_context(
    overrides: ContextOverrides,
    env_value: impl Fn(&str) -> std::result::Result<String, env::VarError>,
) -> Result<ResolvedContext> {
    let ContextOverrides {
        api_url,
        project,
        agent_id,
        agent_run_id,
    } = overrides;
    let api_url = api_url
        .or_else(|| env_value("DISPATCH_API_URL").ok())
        .or_else(|| env_value("DISPATCH_URL").ok())
        .unwrap_or_else(|| DEFAULT_API_URL.to_owned());
    let client = DispatchClient::new(api_url)?;

    let claimed_item_id = optional_i64_env(&env_value, "DISPATCH_CLAIMED_ITEM_ID")?;
    let agent_run_id = match agent_run_id {
        Some(run_id) => Some(run_id),
        None => optional_i64_env(&env_value, "DISPATCH_AGENT_RUN_ID")?,
    };
    let project = project.or_else(|| env_value("DISPATCH_PROJECT").ok());
    let agent_id = agent_id.or_else(|| env_value("DISPATCH_AGENT_ID").ok());

    Ok(ResolvedContext {
        client: client.with_agent_context(agent_id.clone(), agent_run_id),
        project,
        agent_id,
        claimed_item_id,
    })
}

fn optional_i64_env(
    env_value: &impl Fn(&str) -> std::result::Result<String, env::VarError>,
    key: &str,
) -> Result<Option<i64>> {
    let Some(raw) = env_value(key).ok().filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    Ok(Some(
        raw.parse::<i64>()
            .context_with(|| format!("invalid {key} '{raw}'"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use clap::Parser;

    fn env_from<'a>(
        entries: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> std::result::Result<String, env::VarError> + 'a {
        move |key| {
            entries
                .iter()
                .find(|(entry_key, _)| *entry_key == key)
                .map(|(_, value)| value.to_string())
                .ok_or(env::VarError::NotPresent)
        }
    }

    #[test]
    fn context_uses_api_url_project_agent_and_claimed_item_env() {
        let context = resolve_context(
            ContextOverrides::default(),
            env_from(&[
                ("DISPATCH_API_URL", "http://127.0.0.1:4100"),
                ("DISPATCH_PROJECT", "demo"),
                ("DISPATCH_AGENT_ID", "dispatch-run-1"),
                ("DISPATCH_AGENT_RUN_ID", "1"),
                ("DISPATCH_CLAIMED_ITEM_ID", "42"),
            ]),
        )
        .unwrap();

        assert_that!(&(context.client.base_url())).is_equal_to("http://127.0.0.1:4100");
        assert_that!(&(context.project.as_deref())).is_equal_to(Some("demo"));
        assert_that!(&(context.agent_id.as_deref())).is_equal_to(Some("dispatch-run-1"));
        assert_that!(&(context.claimed_item_id)).is_equal_to(Some(42));
    }

    #[test]
    fn explicit_context_overrides_environment() {
        let context = resolve_context(
            ContextOverrides {
                api_url: Some("http://127.0.0.1:5000".to_owned()),
                project: Some("override".to_owned()),
                agent_id: Some("human".to_owned()),
                agent_run_id: Some(9),
            },
            env_from(&[
                ("DISPATCH_API_URL", "http://127.0.0.1:4100"),
                ("DISPATCH_PROJECT", "demo"),
                ("DISPATCH_AGENT_ID", "dispatch-run-1"),
                ("DISPATCH_AGENT_RUN_ID", "invalid"),
            ]),
        )
        .unwrap();

        assert_that!(&(context.client.base_url())).is_equal_to("http://127.0.0.1:5000");
        assert_that!(&(context.project.as_deref())).is_equal_to(Some("override"));
        assert_that!(&(context.agent_id.as_deref())).is_equal_to(Some("human"));
    }

    #[test]
    fn cli_parses_context_options_into_context_overrides() {
        let cli = crate::commands::Cli::parse_from([
            "dispatch",
            "--api-url",
            "http://127.0.0.1:5000",
            "--project",
            "demo",
            "--agent",
            "dispatch-run-1",
            "--agent-run",
            "7",
            "project",
            "list",
        ]);

        let (overrides, _, _) = cli.into_parts();
        assert_that!(&(overrides.api_url.as_deref())).is_equal_to(Some("http://127.0.0.1:5000"));
        assert_that!(&(overrides.project.as_deref())).is_equal_to(Some("demo"));
        assert_that!(&(overrides.agent_id.as_deref())).is_equal_to(Some("dispatch-run-1"));
        assert_that!(&(overrides.agent_run_id)).is_equal_to(Some(7));
    }

    #[test]
    fn item_id_prefers_explicit_value_over_claimed_item_env() {
        let context = ResolvedContext {
            client: DispatchClient::new(DEFAULT_API_URL).unwrap(),
            project: Some("demo".to_owned()),
            agent_id: Some("dispatch-run-1".to_owned()),
            claimed_item_id: Some(42),
        };

        assert_that!(&(context.item_id(Some(124)).unwrap())).is_equal_to(124);
        assert_that!(&(context.item_id(None).unwrap())).is_equal_to(42);
    }

    #[test]
    fn api_url_falls_back_to_dispatch_url_then_local_default() {
        let context = resolve_context(
            ContextOverrides::default(),
            env_from(&[("DISPATCH_URL", "http://127.0.0.1:4200")]),
        )
        .unwrap();
        assert_that!(&(context.client.base_url())).is_equal_to("http://127.0.0.1:4200");

        let context = resolve_context(ContextOverrides::default(), env_from(&[])).unwrap();
        assert_that!(&(context.client.base_url())).is_equal_to(DEFAULT_API_URL);
    }

    #[test]
    fn invalid_api_url_is_rejected_during_context_resolution() {
        let error = resolve_context(
            ContextOverrides {
                api_url: Some("not-a-url".to_owned()),
                ..ContextOverrides::default()
            },
            env_from(&[]),
        )
        .unwrap_err();

        assert_that!(&(error.to_string().contains("invalid Dispatch API URL"))).is_true();
    }
}
