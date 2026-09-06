//! Retire admission only. Preserve historical rows and artifacts for deliberate migration.
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::Statement;
use serde_json::Value;

pub(super) struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260905_000055_retire_legacy_knowledge"
    }
}

fn reserved(value: &str) -> bool {
    matches!(
        value,
        "dispatch:knowledge-maintenance" | "dispatch:knowledge-drift"
    )
}
fn selected_knowledge(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            if object
                .get("column_name")
                .and_then(Value::as_str)
                .is_some_and(|key| reserved(key.trim()))
            {
                // Default ordinary rules explicitly excluded these labels; they must remain active.
                return !matches!(
                    (
                        object.get("operator").and_then(Value::as_str),
                        object
                            .get("value")
                            .and_then(|v| v.get("Bool"))
                            .and_then(Value::as_bool)
                    ),
                    (Some("="), Some(false)) | (Some("!="), Some(true))
                );
            }
            object.values().any(selected_knowledge)
        }
        Value::Array(values) => values.iter().any(selected_knowledge),
        _ => false,
    }
}
fn produces_knowledge(value: &Value) -> bool {
    value
        .get("initial_labels")
        .and_then(Value::as_array)
        .is_some_and(|labels| {
            labels.iter().any(|label| {
                label
                    .get("key")
                    .and_then(Value::as_str)
                    .is_some_and(reserved)
            })
        })
        || value
            .get("deduplication")
            .and_then(|v| v.get("key"))
            .and_then(Value::as_str)
            .is_some_and(|key| matches!(key, "knowledge-maintenance" | "knowledge-drift"))
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        let backend = manager.get_database_backend();
        if backend == sea_orm_migration::sea_orm::DbBackend::Sqlite {
            for name in [
                "trg_projects_source_lineage_insert",
                "trg_projects_source_lineage_update",
            ] {
                connection
                    .execute(Statement::from_string(
                        backend,
                        format!("DROP TRIGGER IF EXISTS {name}"),
                    ))
                    .await?;
            }
        }
        let rows = connection.query_all(Statement::from_string(
            backend,
            "SELECT id, work_item_selector, produced_work_spec_json, concurrency_group, managed_bundle_key, managed_object_key FROM automation_triggers".to_owned(),
        )).await?;
        for row in rows {
            let id: i64 = row.try_get("", "id")?;
            let mut retire = false;
            for field in [
                "concurrency_group",
                "managed_bundle_key",
                "managed_object_key",
            ] {
                let text: Option<String> = row.try_get("", field)?;
                retire |= text.is_some_and(|value| {
                    value.starts_with("knowledge-maintenance")
                        || value.starts_with("knowledge-drift")
                        || value.starts_with("dispatch.knowledge")
                        || value.starts_with("dispatch-knowledge")
                });
            }
            for (field, check) in [
                (
                    "work_item_selector",
                    selected_knowledge as fn(&Value) -> bool,
                ),
                ("produced_work_spec_json", produces_knowledge),
            ] {
                let raw: Option<String> = row.try_get("", field)?;
                if let Some(raw) = raw {
                    if field == "work_item_selector" && raw.trim().is_empty() {
                        continue;
                    }
                    let value: Value = serde_json::from_str(&raw).map_err(|error| {
                        DbErr::Custom(format!(
                            "cannot inspect automation rule {id} {field} during knowledge retirement: {error}"
                        ))
                    })?;
                    retire |= check(&value);
                }
            }
            if retire {
                connection.execute(Statement::from_sql_and_values(
                    backend,
                    "UPDATE automation_triggers SET enabled = FALSE, pending_evaluation_count = 0 WHERE id = $1",
                    [id.into()],
                )).await?;
            }
        }
        // Preserve unfinished legacy items, but prevent ordinary consumers from picking them up
        // after the special knowledge routing logic is removed. Users can explicitly repurpose them.
        connection
            .execute(Statement::from_string(
                backend,
                r#"
            UPDATE label_keys SET built_in = FALSE
            WHERE label_key IN ('dispatch:knowledge-maintenance','dispatch:knowledge-drift')
        "#
                .to_owned(),
            ))
            .await?;
        connection.execute(Statement::from_string(backend, r#"
            UPDATE work_items SET version = version + 1
            WHERE id IN (SELECT work_item_id FROM work_item_labels WHERE label_key IN ('dispatch:knowledge-maintenance','dispatch:knowledge-drift'))
              AND id NOT IN (SELECT work_item_id FROM work_item_labels WHERE label_key = 'dispatch:automation-blocked')
        "#.to_owned())).await?;
        connection.execute(Statement::from_string(backend, r#"
            INSERT INTO work_item_labels (project_id, work_item_id, label_key, label_value, created_at, updated_at)
            SELECT DISTINCT project_id, work_item_id, 'dispatch:automation-blocked', 'Legacy knowledge automation retired', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
            FROM work_item_labels old
            WHERE label_key IN ('dispatch:knowledge-maintenance','dispatch:knowledge-drift')
              AND NOT EXISTS (SELECT 1 FROM work_item_labels blocked WHERE blocked.work_item_id = old.work_item_id AND blocked.label_key = 'dispatch:automation-blocked')
        "#.to_owned())).await?;
        connection.execute(Statement::from_string(backend, r#"
            UPDATE agent_runs SET status = 'failed', result_summary = 'Interrupted by knowledge subsystem replacement', finished_at = CURRENT_TIMESTAMP
            WHERE status = 'running' AND (purpose IN ('knowledge_cycle','knowledge_answer') OR run_kind = 'knowledge_answer')
        "#.to_owned())).await?;
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Downgrading never silently resumes retired model work.
        Ok(())
    }
}
