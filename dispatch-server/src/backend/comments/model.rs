pub(crate) enum CommentTarget<'a> {
    ProjectItem { project: &'a str, item_id: i64 },
    Item(i64),
}

pub(crate) struct ItemScope {
    pub(crate) project_id: i64,
    pub(crate) project_name: String,
}
