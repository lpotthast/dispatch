pub(crate) struct StateFields {
    pub(crate) identifier: String,
    pub(crate) name: String,
    pub(crate) position: i64,
}
impl StateFields {
    pub(crate) fn validate(mut self) -> rootcause::Result<Self> {
        self.identifier = super::policy::normalize_identifier(self.identifier)?;
        self.name = super::policy::normalize_name(self.name)?;

        Ok(self)
    }
}
