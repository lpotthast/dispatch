//! Structural regressions for the application and the extracted service boundaries.
use assertr::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            paths.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            paths.push(path);
        }
    }
    paths
}

#[test]
fn backend_dependencies_do_not_point_to_frontend_rendering() {
    let backend = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
    let violations = rust_sources(&backend)
        .into_iter()
        .filter(|path| {
            // HTTP assembly mounts the Leptos shell; domain code has no presentation dependency.
            !matches!(
                path.file_name().unwrap().to_str().unwrap(),
                "http.rs" | "architecture_tests.rs"
            ) && fs::read_to_string(path).unwrap().contains("frontend::")
        })
        .collect::<Vec<_>>();
    assert_that!(&violations).is_empty();
}

#[test]
fn application_state_events_and_execution_url_have_no_process_global_lookup() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let violations = rust_sources(&source_root)
        .into_iter()
        .filter(|path| {
            if path.file_name().unwrap() == "architecture_tests.rs" {
                return false;
            }
            let source = fs::read_to_string(path).unwrap();
            [
                "static APP_STATE",
                "static EVENT_BUS",
                "static SERVER_API_URL",
                "static MANAGED_CODEX_HOME_OPERATION",
                "static WRITES",
                "app_state::app_state(",
            ]
            .iter()
            .any(|pattern| source.contains(pattern))
        })
        .collect::<Vec<_>>();
    assert_that!(&violations).is_empty();
}

#[test]
fn extracted_services_do_not_look_up_application_state_or_query_the_orm() {
    let backend = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
    for service in [
        "attribution/service.rs",
        "knowledge/service.rs",
        "knowledge/queries.rs",
        "knowledge/jobs/service.rs",
        "knowledge/jobs/processing.rs",
        "knowledge/jobs/execution.rs",
        "knowledge/jobs/publication/service.rs",
        "knowledge/jobs/worker.rs",
        "runs/control.rs",
        "execution/service.rs",
        "projects/service.rs",
        "board/queries.rs",
        "operator/queries.rs",
        "projects/settings.rs",
        "projects/model.rs",
        "projects/worker.rs",
        "projects/deletion.rs",
        "items/service.rs",
        "items/policy.rs",
        "items/groups/service.rs",
        "items/groups/policy.rs",
        "items/groups/model.rs",
        "items/events/model.rs",
        "items/claims/service.rs",
        "items/claims/model.rs",
        "items/claims/policy.rs",
        "runs/launch/model.rs",
        "runs/launch/policy.rs",
        "items/labels/service.rs",
        "items/labels/policy.rs",
        "items/labels/mutations.rs",
        "items/labels/workflow.rs",
        "items/creation/service.rs",
        "items/creation/policy.rs",
        "automation/production/service.rs",
        "automation/supervisor.rs",
        "automation/rules/service.rs",
        "automation/rules/model.rs",
        "automation/rules/configuration.rs",
        "automation/rules/policy.rs",
        "automation/revisions/service.rs",
        "automation/revisions/model.rs",
        "comments/service.rs",
        "comments/model.rs",
        "relationships/service.rs",
        "relationships/policy.rs",
        "runs/service.rs",
        "runs/model.rs",
        "runs/admission/service.rs",
        "runs/admission/policy.rs",
        "runs/admission/model.rs",
    ] {
        let source = fs::read_to_string(backend.join(service)).unwrap();
        let source = source.split("#[cfg(test)]").next().unwrap();
        for forbidden in [
            "AppState",
            "expect_context",
            "use_context",
            "sea_orm",
            "entities::",
            "HeaderMap",
        ] {
            assert_that!(&source.contains(forbidden)).is_false();
        }
    }
}

#[test]
fn shared_execution_cannot_call_item_or_knowledge_job_workflows() {
    let source = [
        include_str!("execution/service.rs"),
        include_str!("execution/runtime.rs"),
        include_str!("execution/model.rs"),
    ]
    .join("\n");
    for forbidden in [
        "item_claims::",
        "items::",
        "knowledge::jobs",
        "storage::Store",
        "store.db()",
    ] {
        assert_that!(&source.contains(forbidden)).is_false();
    }
}

#[test]
fn project_lifecycle_hooks_have_no_workflow_writes_or_event_delivery() {
    let source = include_str!("projects/transport/crud.rs");
    let hooks = source
        .split("impl CrudLifetime<CrudProjectResource> for ProjectLifetime")
        .nth(1)
        .unwrap()
        .split("pub struct CrudProjectResource")
        .next()
        .unwrap();
    for forbidden in [
        ".create(",
        ".edit(",
        ".delete",
        ".insert(",
        ".update(",
        ".publish_",
        "store.",
        "transaction",
        "ensure_default",
        "ensure_built_in",
    ] {
        assert_that!(&hooks.contains(forbidden)).is_false();
    }
}

#[test]
fn project_domain_keeps_queries_and_storage_encoding_in_its_repository() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend/projects");
    let violations = rust_sources(&directory)
        .into_iter()
        .filter(|path| {
            let relative = path.strip_prefix(&directory).unwrap();
            if relative.starts_with("repository")
                || relative.starts_with("transport")
                || matches!(
                    path.file_name().unwrap().to_str().unwrap(),
                    "tests.rs" | "repository.rs" | "transport.rs"
                )
            {
                return false;
            }
            let source = fs::read_to_string(path).unwrap();
            let source = source.split("#[cfg(test)]").next().unwrap();
            ["sea_orm", "entities::", ".connection()"]
                .iter()
                .any(|pattern| source.contains(pattern))
        })
        .collect::<Vec<_>>();
    assert_that!(&violations).is_empty();
}

#[test]
fn discussion_domains_keep_database_access_in_persistence_adapters() {
    let backend = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
    for domain in [
        "comments",
        "relationships",
        "items/states",
        "board/lanes",
        "items/labels/catalog",
        "items/claims",
        "runs/launch",
        "runs/queries",
        "execution/tools",
        "execution/codex",
        "execution/workspaces",
        "automation/personalities",
        "automation/bundles",
        "automation/launch",
        "automation/scheduling",
        "automation/routing",
        "automation/postconditions",
    ] {
        for path in rust_sources(&backend.join(domain)) {
            let relative = path.strip_prefix(backend.join(domain)).unwrap();
            if relative.starts_with("repository")
                || relative.starts_with("transport")
                || matches!(
                    path.file_name().unwrap().to_str().unwrap(),
                    "tests.rs" | "repository.rs" | "transport.rs"
                )
            {
                continue;
            }
            let source = fs::read_to_string(path).unwrap();
            let source = source.split("#[cfg(test)]").next().unwrap();
            for forbidden in [
                "sea_orm",
                "entities::",
                ".connection()",
                "storage::Store",
                "AppState",
                "HeaderMap",
            ] {
                assert_that!(&source.contains(forbidden)).is_false();
            }
        }
    }
}

#[test]
fn comment_crudkit_hooks_only_adapt_validation() {
    let source = include_str!("comments/transport/crud.rs");
    let hooks = source
        .split("impl CrudLifetime<CrudCommentResource> for CommentLifetime")
        .nth(1)
        .unwrap();
    for forbidden in [
        ".add(",
        ".update(",
        ".delete(",
        ".insert(",
        ".publish_",
        ".begin(",
        "store.",
    ] {
        assert_that!(&hooks.contains(forbidden)).is_false();
    }
}

#[test]
fn item_crudkit_hooks_only_adapt_validation() {
    let source = include_str!("items/transport/crud.rs");
    let hooks = source
        .split("impl CrudLifetime<CrudWorkItemResource> for WorkItemLifetime")
        .nth(1)
        .unwrap()
        .split("fn validate_work_item_text")
        .next()
        .unwrap();
    for forbidden in [
        ".create(",
        ".update(",
        ".delete(",
        ".insert(",
        ".publish_",
        ".begin(",
        "store.",
    ] {
        assert_that!(&hooks.contains(forbidden)).is_false();
    }
}

#[test]
fn authored_configuration_hooks_only_adapt_validation() {
    for (source, resource, lifetime) in [
        (
            include_str!("items/states/transport.rs"),
            "CrudWorkItemStateResource",
            "WorkItemStateLifetime",
        ),
        (
            include_str!("board/lanes/transport.rs"),
            "CrudSwimLaneResource",
            "SwimLaneLifetime",
        ),
    ] {
        let marker = format!("impl CrudLifetime<{resource}> for {lifetime}");
        let hooks = source.split(&marker).nth(1).unwrap();
        for forbidden in [
            ".create(",
            ".update(",
            ".delete(",
            ".insert(",
            ".publish_",
            ".begin(",
            "store.",
        ] {
            assert_that!(&hooks.contains(forbidden)).is_false();
        }
    }
}

#[test]
fn label_catalog_hooks_only_adapt_validation() {
    let source = include_str!("items/labels/catalog/transport.rs");
    let hooks = source
        .split("impl CrudLifetime<CrudLabelKeyResource> for LabelKeyLifetime")
        .nth(1)
        .unwrap();
    for forbidden in [
        ".create(",
        ".update(",
        ".delete(",
        ".insert(",
        ".publish_",
        ".begin(",
        "store.",
        "forget_if_unused",
    ] {
        assert_that!(&hooks.contains(forbidden)).is_false();
    }
}

#[test]
fn personality_crudkit_hooks_only_adapt_validation() {
    let source = include_str!("automation/personalities/transport/crud.rs");
    let hooks = source
        .split("impl CrudLifetime<CrudPersonalityResource> for PersonalityLifetime")
        .nth(1)
        .unwrap()
        .split("pub struct CrudPersonalityResource")
        .next()
        .unwrap();
    for forbidden in [
        ".create(",
        ".update(",
        ".delete(",
        ".restore(",
        ".detach(",
        ".insert(",
        ".publish_",
        "record_revision",
        "store.",
    ] {
        assert_that!(&hooks.contains(forbidden)).is_false();
    }
}

#[test]
fn rule_crudkit_hooks_only_adapt_validation() {
    let source = include_str!("automation/rules/transport/crud.rs");
    let hooks = source
        .split("impl CrudLifetime<CrudAutomationTriggerResource> for AutomationTriggerLifetime")
        .nth(1)
        .unwrap()
        .split("pub struct CrudAutomationTriggerResource")
        .next()
        .unwrap();
    for forbidden in [
        ".create(",
        ".update(",
        ".delete(",
        ".restore(",
        ".detach(",
        ".insert(",
        ".publish_",
        "record_revision",
        "store.",
    ] {
        assert_that!(&hooks.contains(forbidden)).is_false();
    }
}

#[test]
fn document_service_and_storage_do_not_own_job_execution_or_workflow_locks() {
    let documents = include_str!("knowledge/service.rs");
    for forbidden in ["jobs::", "PassAgent", "ProcessSession", "AgentExecution"] {
        assert_that!(&documents.contains(forbidden)).is_false();
    }
    let storage = include_str!("storage.rs");
    for forbidden in [
        "Mutex",
        "lock_knowledge_jobs",
        "admission_lock",
        "producer_lock",
    ] {
        assert_that!(&storage.contains(forbidden)).is_false();
    }
}
