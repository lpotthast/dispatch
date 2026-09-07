use dispatch_types::AuthorType;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EventAttribution<'a> {
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<&'a str>,
    pub agent_run_id: Option<i64>,
}

pub(crate) fn agent_event_attribution(agent_id: &str) -> EventAttribution<'_> {
    EventAttribution {
        actor_type: Some(AuthorType::Agent),
        actor_id: Some(agent_id),
        agent_run_id: crate::backend::execution::identity::parse_dispatch_run_agent_id(agent_id),
    }
}
