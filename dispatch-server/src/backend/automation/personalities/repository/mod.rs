use super::policy::DEFAULT_PERSONALITY_NAME;
use crate::backend::{
    automation::revisions::model::RevisionActor,
    entities::{
        automation_trigger,
        personality::{self, Personality, PersonalityActiveModel, PersonalityModel},
    },
    storage::{Transaction, utc_now},
};
use dispatch_types::{PersonalityView, RevisionChangeOperation};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel,
    QueryFilter, QueryOrder, QuerySelect,
};
pub(crate) mod revisions;
impl From<PersonalityModel> for PersonalityView {
    fn from(personality: PersonalityModel) -> Self {
        Self {
            id: personality.id,
            project_id: personality.project_id,
            name: personality.name,
            personality_description: personality.personality_description,
            current_revision_id: personality.current_revision_id,
            managed_bundle_key: personality.managed_bundle_key,
            managed_object_key: personality.managed_object_key,
            created_at: personality.created_at,
            updated_at: personality.updated_at,
        }
    }
}

pub(crate) async fn ensure_default_personality_in_conn<C>(
    conn: &C,
    project_id: i64,
) -> Result<PersonalityModel>
where
    C: ConnectionTrait,
{
    if let Some(existing) = Personality::find()
        .filter(personality::Column::ProjectId.eq(project_id))
        .filter(personality::Column::Name.eq(DEFAULT_PERSONALITY_NAME))
        .limit(1)
        .one(conn)
        .await
        .context("failed to load default personality")?
    {
        return Ok(existing);
    }

    let now = utc_now();
    let personality = PersonalityActiveModel {
        project_id: Set(project_id),
        name: Set(DEFAULT_PERSONALITY_NAME.to_owned()),
        personality_description: Set(String::new()),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("failed to create default personality")?;
    revisions::record_in_conn(
        conn,
        &personality,
        RevisionChangeOperation::Create,
        &RevisionActor::default(),
    )
    .await?;
    Personality::find_by_id(personality.id)
        .one(conn)
        .await
        .context("failed to reload default personality")?
        .ok_or_else(|| report!("created default personality disappeared"))
}

pub(crate) async fn default_personality_id_in_conn<C>(conn: &C, project_id: i64) -> Result<i64>
where
    C: ConnectionTrait,
{
    Ok(ensure_default_personality_in_conn(conn, project_id)
        .await?
        .id)
}

pub(crate) fn encode(record: PersonalityView) -> PersonalityModel {
    PersonalityModel {
        id: record.id,
        project_id: record.project_id,
        name: record.name,
        personality_description: record.personality_description,
        current_revision_id: record.current_revision_id,
        managed_bundle_key: record.managed_bundle_key,
        managed_object_key: record.managed_object_key,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}
pub(crate) struct PersonalityRepository;
impl PersonalityRepository {
    pub(crate) async fn find_named_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        name: &str,
    ) -> Result<Option<PersonalityView>> {
        Ok(Personality::find()
            .filter(personality::Column::ProjectId.eq(project_id))
            .filter(
                sea_orm::Condition::any()
                    .add(personality::Column::Name.eq(name))
                    .add(personality::Column::ManagedObjectKey.eq(name)),
            )
            .one(transaction.connection())
            .await
            .context("failed to resolve automation personality")?
            .map(Into::into))
    }

    pub(crate) async fn revisions_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<Vec<dispatch_types::PersonalityRevisionView>> {
        revisions::list_in(transaction, project_id, id).await
    }
    pub(crate) async fn revision_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
        revision_id: i64,
    ) -> Result<dispatch_types::PersonalityRevisionView> {
        revisions::get_in(transaction, project_id, id, revision_id).await
    }

    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<PersonalityView>> {
        Ok(Personality::find()
            .filter(personality::Column::ProjectId.eq(project_id))
            .order_by_asc(personality::Column::Name)
            .all(transaction.connection())
            .await
            .context("failed to list personalities")?
            .into_iter()
            .map(Into::into)
            .collect())
    }
    pub(crate) async fn find_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        reference: &str,
    ) -> Result<Option<PersonalityView>> {
        let mut condition = sea_orm::Condition::any()
            .add(personality::Column::Name.eq(reference))
            .add(personality::Column::ManagedObjectKey.eq(reference));
        if let Ok(id) = reference.parse::<i64>() {
            condition = condition.add(personality::Column::Id.eq(id));
        }
        Ok(Personality::find()
            .filter(personality::Column::ProjectId.eq(project_id))
            .filter(condition)
            .one(transaction.connection())
            .await
            .context("failed to load personality")?
            .map(Into::into))
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<PersonalityView> {
        Personality::find_by_id(id)
            .filter(personality::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load personality")?
            .map(Into::into)
            .ok_or_else(|| report!("personality {id} does not exist in this project"))
    }
    pub(crate) async fn name_exists_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        name: &str,
        except: Option<i64>,
    ) -> Result<bool> {
        let mut query = Personality::find()
            .filter(personality::Column::ProjectId.eq(project_id))
            .filter(personality::Column::Name.eq(name));
        if let Some(id) = except {
            query = query.filter(personality::Column::Id.ne(id));
        }
        Ok(query
            .one(transaction.connection())
            .await
            .context("failed to check personality name")?
            .is_some())
    }
    pub(crate) async fn referencing_rule_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<Option<String>> {
        Ok(automation_trigger::Entity::find()
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .filter(automation_trigger::Column::PersonalityId.eq(id))
            .limit(1)
            .one(transaction.connection())
            .await
            .context("failed to check personality references")?
            .map(|rule| rule.name))
    }
    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        record: PersonalityView,
    ) -> Result<PersonalityView> {
        let mut active = encode(record).into_active_model().reset_all();
        active.id = Default::default();
        Ok(active
            .insert(transaction.connection())
            .await
            .context("failed to create personality")?
            .into())
    }
    pub(crate) async fn save_in(
        &self,
        transaction: &Transaction,
        record: PersonalityView,
    ) -> Result<PersonalityView> {
        Ok(encode(record)
            .into_active_model()
            .reset_all()
            .update(transaction.connection())
            .await
            .context("failed to update personality")?
            .into())
    }
    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<u64> {
        Ok(Personality::delete_by_id(id)
            .filter(personality::Column::ProjectId.eq(project_id))
            .exec(transaction.connection())
            .await
            .context("failed to delete personality")?
            .rows_affected)
    }
    pub(crate) async fn record_revision_in(
        &self,
        transaction: &Transaction,
        mut record: PersonalityView,
        operation: RevisionChangeOperation,
    ) -> Result<PersonalityView> {
        record.current_revision_id = Some(
            revisions::record_in_conn(
                transaction.connection(),
                &encode(record.clone()),
                operation,
                &RevisionActor::default(),
            )
            .await?,
        );
        Ok(record)
    }
}
