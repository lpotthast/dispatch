use std::fmt::Display;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Endpoint {
    path: String,
    has_query: bool,
}

impl Endpoint {
    pub(super) fn project(project: &str, route: &'static str) -> Self {
        Self::new("/api/projects").segment(project).extend(route)
    }

    pub(super) fn operator_project(project: &str, route: &'static str) -> Self {
        Self::new("/operator/api/projects")
            .segment(project)
            .extend(route)
    }

    pub(super) fn extend(mut self, suffix: &'static str) -> Self {
        if suffix.is_empty() {
            return self;
        }
        for segment in suffix.split('/') {
            debug_assert!(!segment.is_empty());
            self.push_segment(segment);
        }
        self
    }

    pub(super) fn segment(mut self, value: impl Display) -> Self {
        let value = value.to_string();
        self.push_segment(&value);
        self
    }

    fn push_segment(&mut self, value: &str) {
        self.path.push('/');
        self.path.push_str(&urlencoding::encode(value));
    }

    pub(super) fn query(mut self, key: &str, value: impl Display) -> Self {
        let value = value.to_string();
        self.path.push(if self.has_query { '&' } else { '?' });
        self.has_query = true;
        self.path.push_str(&urlencoding::encode(key));
        self.path.push('=');
        self.path.push_str(&urlencoding::encode(&value));
        self
    }

    fn new(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            has_query: false,
        }
    }
}

impl AsRef<str> for Endpoint {
    fn as_ref(&self) -> &str {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn encodes_each_dynamic_path_segment() {
        assert_that!(&(Endpoint::project("demo", "").as_ref())).is_equal_to("/api/projects/demo");

        let endpoint =
            Endpoint::project("docs/tools", "knowledge/nodes").segment("architecture & API");

        assert_that!(&(endpoint.as_ref()))
            .is_equal_to("/api/projects/docs%2Ftools/knowledge/nodes/architecture%20%26%20API");
    }

    #[test]
    fn appends_and_encodes_query_parameters() {
        let endpoint = Endpoint::operator_project("one two", "automation/evaluations")
            .query("trigger id", 17)
            .query("filter", "ready & waiting");

        assert_that!(&(endpoint.as_ref())).is_equal_to(
            "/operator/api/projects/one%20two/automation/evaluations?trigger%20id=17&filter=ready%20%26%20waiting",
        );
    }
}
