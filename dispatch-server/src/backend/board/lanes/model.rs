pub(crate) struct LaneFields {
    pub(crate) identifier: String,
    pub(crate) name: String,
    pub(crate) position: i64,
    pub(crate) filter: crudkit_core::condition::Condition,
    pub(crate) item_order: dispatch_types::SwimLaneItemOrder,
    pub(crate) can_create_items: bool,
}
impl LaneFields {
    pub(crate) fn validate(mut self) -> rootcause::Result<Self> {
        self.identifier = super::policy::normalize_identifier(self.identifier)?;
        self.name = super::policy::normalize_name(self.name)?;
        crate::backend::items::labels::conditions::validate_condition(&self.filter)?;
        Ok(self)
    }
}
