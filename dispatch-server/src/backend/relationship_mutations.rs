use rootcause::{Result, prelude::*};

use crate::{
    backend::entities::work_item_relationship::WorkItemRelationshipModel,
    shared::view_models::{WorkItemEventType, WorkItemRelationshipDirection},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RelationshipEndpoints {
    pub(crate) source_work_item_id: i64,
    pub(crate) target_work_item_id: i64,
}

impl RelationshipEndpoints {
    pub(crate) fn new(source_work_item_id: i64, target_work_item_id: i64) -> Result<Self> {
        if source_work_item_id == target_work_item_id {
            bail!("relationship source and target work items must differ");
        }
        Ok(Self {
            source_work_item_id,
            target_work_item_id,
        })
    }

    pub(crate) fn from_relationship(relationship: &WorkItemRelationshipModel) -> Result<Self> {
        Self::new(
            relationship.source_work_item_id,
            relationship.target_work_item_id,
        )
    }

    pub(crate) fn ensure_touches_requested_item(
        self,
        requested_item_id: Option<i64>,
        relationship_id: i64,
    ) -> Result<()> {
        let Some(item_id) = requested_item_id else {
            return Ok(());
        };
        if self.source_work_item_id == item_id || self.target_work_item_id == item_id {
            return Ok(());
        }
        bail!(
            "relationship {} does not touch item {}",
            relationship_id,
            item_id
        )
    }

    pub(crate) fn direction_for_item(self, item_id: i64) -> WorkItemRelationshipDirection {
        if self.source_work_item_id == item_id {
            WorkItemRelationshipDirection::Outgoing
        } else {
            WorkItemRelationshipDirection::Incoming
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RelationshipEvent {
    pub(crate) event_type: WorkItemEventType,
    pub(crate) source_body: String,
    pub(crate) target_body: String,
}

impl RelationshipEvent {
    fn same(event_type: WorkItemEventType, body: String) -> Self {
        Self {
            event_type,
            source_body: body.clone(),
            target_body: body,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreateRelationshipMutation {
    endpoints: RelationshipEndpoints,
    kind: String,
}

impl CreateRelationshipMutation {
    pub(crate) fn new(
        source_work_item_id: i64,
        target_work_item_id: i64,
        kind: String,
    ) -> Result<Self> {
        let kind = normalize_kind(kind)?;
        Ok(Self {
            endpoints: RelationshipEndpoints::new(source_work_item_id, target_work_item_id)?,
            kind,
        })
    }

    pub(crate) fn endpoints(&self) -> RelationshipEndpoints {
        self.endpoints
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn created_event(&self) -> RelationshipEvent {
        RelationshipEvent {
            event_type: WorkItemEventType::RelationshipCreated,
            source_body: format!(
                "Created relationship #{} {} #{}",
                self.endpoints.source_work_item_id,
                self.kind(),
                self.endpoints.target_work_item_id
            ),
            target_body: format!(
                "Created incoming relationship from #{}: {}",
                self.endpoints.source_work_item_id,
                self.kind()
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UpdateRelationshipMutation {
    relationship_id: i64,
    endpoints: RelationshipEndpoints,
    kind: String,
}

impl UpdateRelationshipMutation {
    pub(crate) fn new(
        relationship: &WorkItemRelationshipModel,
        requested_item_id: Option<i64>,
        kind: String,
    ) -> Result<Self> {
        let kind = normalize_kind(kind)?;
        let endpoints = RelationshipEndpoints::from_relationship(relationship)?;
        endpoints.ensure_touches_requested_item(requested_item_id, relationship.id)?;
        Ok(Self {
            relationship_id: relationship.id,
            endpoints,
            kind,
        })
    }

    pub(crate) fn endpoints(&self) -> RelationshipEndpoints {
        self.endpoints
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn duplicate_exclusion(&self) -> Option<i64> {
        Some(self.relationship_id)
    }

    pub(crate) fn updated_event(&self) -> RelationshipEvent {
        RelationshipEvent::same(
            WorkItemEventType::RelationshipUpdated,
            format!(
                "Updated relationship #{} kind to {}",
                self.relationship_id,
                self.kind()
            ),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeleteRelationshipMutation {
    relationship_id: i64,
    endpoints: RelationshipEndpoints,
    kind: String,
}

impl DeleteRelationshipMutation {
    pub(crate) fn new(
        relationship: &WorkItemRelationshipModel,
        requested_item_id: Option<i64>,
    ) -> Result<Self> {
        let endpoints = RelationshipEndpoints::from_relationship(relationship)?;
        endpoints.ensure_touches_requested_item(requested_item_id, relationship.id)?;
        Ok(Self {
            relationship_id: relationship.id,
            endpoints,
            kind: relationship.kind.clone(),
        })
    }

    pub(crate) fn endpoints(&self) -> RelationshipEndpoints {
        self.endpoints
    }

    pub(crate) fn deleted_event(&self) -> RelationshipEvent {
        RelationshipEvent::same(
            WorkItemEventType::RelationshipDeleted,
            format!(
                "Deleted relationship #{}: #{} {} #{}",
                self.relationship_id,
                self.endpoints.source_work_item_id,
                self.kind,
                self.endpoints.target_work_item_id
            ),
        )
    }
}

fn normalize_kind(kind: String) -> Result<String> {
    let kind = kind.trim().to_owned();
    if kind.is_empty() {
        bail!("relationship kind cannot be empty");
    }
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relationship() -> WorkItemRelationshipModel {
        WorkItemRelationshipModel {
            id: 9,
            project_id: 4,
            source_work_item_id: 42,
            target_work_item_id: 18,
            kind: "blocks".to_owned(),
            created_at: "2026-06-19T00:00:00Z".to_owned(),
            updated_at: "2026-06-19T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn create_mutation_normalizes_kind_and_builds_endpoint_events() {
        let mutation = CreateRelationshipMutation::new(42, 18, " follows ".to_owned()).unwrap();

        assert_eq!(mutation.kind(), "follows");
        assert_eq!(
            mutation.endpoints(),
            RelationshipEndpoints {
                source_work_item_id: 42,
                target_work_item_id: 18,
            }
        );
        assert_eq!(
            mutation.created_event(),
            RelationshipEvent {
                event_type: WorkItemEventType::RelationshipCreated,
                source_body: "Created relationship #42 follows #18".to_owned(),
                target_body: "Created incoming relationship from #42: follows".to_owned(),
            }
        );
    }

    #[test]
    fn create_mutation_rejects_self_link_and_empty_kind() {
        let self_link = CreateRelationshipMutation::new(42, 42, "blocks".to_owned()).unwrap_err();
        assert!(self_link.to_string().contains("must differ"));

        let empty_kind = CreateRelationshipMutation::new(42, 18, " ".to_owned()).unwrap_err();
        assert!(empty_kind.to_string().contains("kind cannot be empty"));
    }

    #[test]
    fn update_mutation_validates_item_scope_and_excludes_current_relationship() {
        let mutation =
            UpdateRelationshipMutation::new(&relationship(), Some(42), " unblocks ".to_owned())
                .unwrap();

        assert_eq!(mutation.kind(), "unblocks");
        assert_eq!(mutation.duplicate_exclusion(), Some(9));
        assert_eq!(
            mutation.updated_event(),
            RelationshipEvent::same(
                WorkItemEventType::RelationshipUpdated,
                "Updated relationship #9 kind to unblocks".to_owned(),
            )
        );

        let err = UpdateRelationshipMutation::new(&relationship(), Some(7), "unblocks".to_owned())
            .unwrap_err();
        assert!(err.to_string().contains("does not touch item"));
    }

    #[test]
    fn delete_mutation_keeps_deleted_relationship_snapshot_for_events() {
        let mutation = DeleteRelationshipMutation::new(&relationship(), Some(18)).unwrap();

        assert_eq!(
            mutation.deleted_event(),
            RelationshipEvent::same(
                WorkItemEventType::RelationshipDeleted,
                "Deleted relationship #9: #42 blocks #18".to_owned(),
            )
        );

        let err = DeleteRelationshipMutation::new(&relationship(), Some(7)).unwrap_err();
        assert!(err.to_string().contains("does not touch item"));
    }

    #[test]
    fn endpoint_direction_is_relative_to_the_requested_item() {
        let endpoints = RelationshipEndpoints::new(42, 18).unwrap();

        assert_eq!(
            endpoints.direction_for_item(42),
            WorkItemRelationshipDirection::Outgoing
        );
        assert_eq!(
            endpoints.direction_for_item(18),
            WorkItemRelationshipDirection::Incoming
        );
    }
}
