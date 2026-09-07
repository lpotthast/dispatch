use dispatch_types::{PostconditionFailureView, SemanticPostconditionStatus};
#[derive(Clone, Debug)]
pub(crate) struct SemanticEvaluation {
    pub(crate) status: SemanticPostconditionStatus,
    pub(crate) failures: Vec<PostconditionFailureView>,
}
