use dispatch_types::AutomationRunMutability;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RunningRunCounts {
    pub(crate) mutating: i64,
    pub(crate) read_only: i64,
}

impl RunningRunCounts {
    pub(crate) fn total(self) -> i64 {
        self.mutating.saturating_add(self.read_only)
    }

    pub(super) fn for_mutability(self, mutability: AutomationRunMutability) -> i64 {
        match mutability {
            AutomationRunMutability::Mutating => self.mutating,
            AutomationRunMutability::ReadOnly => self.read_only,
        }
    }
}
