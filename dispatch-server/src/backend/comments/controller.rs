//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct CommentController {
    pub(super) comments: Arc<crate::backend::comments::service::CommentService>,
}
impl CommentController {
    pub(crate) fn new(comments: Arc<crate::backend::comments::service::CommentService>) -> Self {
        Self { comments }
    }
}
