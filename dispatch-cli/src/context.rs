use std::env;

use dispatch_api_client::DispatchClient;
use rootcause::{Result, option_ext::OptionExt, prelude::*};

const DEFAULT_API_URL: &str = "http://127.0.0.1:4000";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ContextOverrides {
    pub api_url: Option<String>,
    pub project: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedContext {
    api_url: String,
    project: Option<String>,
    agent_id: Option<String>,
    claimed_item_id: Option<i64>,
}

impl ResolvedContext {
    pub(crate) fn client(&self) -> DispatchClient {
        DispatchClient::new(self.api_url.clone())
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
    overrides: &ContextOverrides,
    env_value: impl Fn(&str) -> std::result::Result<String, env::VarError>,
) -> Result<ResolvedContext> {
    let api_url = overrides
        .api_url
        .clone()
        .or_else(|| env_value("DISPATCH_API_URL").ok())
        .or_else(|| env_value("DISPATCH_URL").ok())
        .unwrap_or_else(|| DEFAULT_API_URL.to_owned());
    if api_url.trim().is_empty() {
        bail!("Dispatch API URL cannot be empty");
    }

    let claimed_item_id = match env_value("DISPATCH_CLAIMED_ITEM_ID").ok() {
        Some(raw) if !raw.trim().is_empty() => Some(
            raw.parse::<i64>()
                .context_with(|| format!("invalid DISPATCH_CLAIMED_ITEM_ID '{raw}'"))?,
        ),
        _ => None,
    };

    Ok(ResolvedContext {
        api_url,
        project: overrides
            .project
            .clone()
            .or_else(|| env_value("DISPATCH_PROJECT").ok()),
        agent_id: overrides
            .agent_id
            .clone()
            .or_else(|| env_value("DISPATCH_AGENT_ID").ok()),
        claimed_item_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            &ContextOverrides::default(),
            env_from(&[
                ("DISPATCH_API_URL", "http://127.0.0.1:4100"),
                ("DISPATCH_PROJECT", "demo"),
                ("DISPATCH_AGENT_ID", "dispatch-run-1"),
                ("DISPATCH_CLAIMED_ITEM_ID", "42"),
            ]),
        )
        .unwrap();

        assert_eq!(context.api_url, "http://127.0.0.1:4100");
        assert_eq!(context.project.as_deref(), Some("demo"));
        assert_eq!(context.agent_id.as_deref(), Some("dispatch-run-1"));
        assert_eq!(context.claimed_item_id, Some(42));
    }

    #[test]
    fn explicit_context_overrides_environment() {
        let context = resolve_context(
            &ContextOverrides {
                api_url: Some("http://127.0.0.1:5000".to_owned()),
                project: Some("override".to_owned()),
                agent_id: Some("human".to_owned()),
            },
            env_from(&[
                ("DISPATCH_API_URL", "http://127.0.0.1:4100"),
                ("DISPATCH_PROJECT", "demo"),
                ("DISPATCH_AGENT_ID", "dispatch-run-1"),
            ]),
        )
        .unwrap();

        assert_eq!(context.api_url, "http://127.0.0.1:5000");
        assert_eq!(context.project.as_deref(), Some("override"));
        assert_eq!(context.agent_id.as_deref(), Some("human"));
    }

    #[test]
    fn item_id_prefers_explicit_value_over_claimed_item_env() {
        let context = ResolvedContext {
            api_url: DEFAULT_API_URL.to_owned(),
            project: Some("demo".to_owned()),
            agent_id: Some("dispatch-run-1".to_owned()),
            claimed_item_id: Some(42),
        };

        assert_eq!(context.item_id(Some(124)).unwrap(), 124);
        assert_eq!(context.item_id(None).unwrap(), 42);
    }

    #[test]
    fn api_url_falls_back_to_dispatch_url_then_local_default() {
        let context = resolve_context(
            &ContextOverrides::default(),
            env_from(&[("DISPATCH_URL", "http://127.0.0.1:4200")]),
        )
        .unwrap();
        assert_eq!(context.api_url, "http://127.0.0.1:4200");

        let context = resolve_context(&ContextOverrides::default(), env_from(&[])).unwrap();
        assert_eq!(context.api_url, DEFAULT_API_URL);
    }
}
