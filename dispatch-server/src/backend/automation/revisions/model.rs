use dispatch_types::AuthorType;
#[derive(Clone, Debug, Default)]
pub(crate) struct RevisionActor {
    pub(crate) actor_type: Option<AuthorType>,
    pub(crate) actor_id: Option<String>,
}
