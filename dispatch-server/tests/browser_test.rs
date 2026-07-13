#![cfg(not(target_arch = "wasm32"))]

use std::{
    borrow::Cow,
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use assertr::prelude::*;
use browser_test::thirtyfour::{By, ChromiumLikeCapabilities, Key, WebDriver};
use browser_test::{
    BrowserTest, BrowserTestFailurePolicy, BrowserTestParallelism, BrowserTestRunner,
    BrowserTestVisibility, BrowserTests, BrowserTimeouts, ChromeBinary, PauseConfig, async_trait,
};
use leptos_browser_test::{LeptosTestApp, LeptosTestAppConfig, Report, ResultExt, bail};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn browser_tests() -> Result<(), Report> {
    tracing_subscriber::fmt().init();

    let app = DispatchTestApp::start().await?;
    let browser_visibility = BrowserTestVisibility::from_env();
    let run_chrome_single_process = browser_visibility.resolve().is_headless();

    let run_result = BrowserTestRunner::new()
        // Headless Shell is a command-line Chrome-for-Testing artifact, not the macOS Chrome .app
        // bundle. That avoids LaunchServices / WindowServer app-registration calls that are
        // blocked by the default Codex SDK sandbox before WebDriver can create a session. Visible
        // browser-test runs still use regular Chrome because Headless Shell cannot show a window.
        .with_headless_chrome_binary(ChromeBinary::ChromeHeadlessShell)
        .with_chrome_capabilities(move |caps| {
            // Chrome's process sandbox can fail in nested/managed CI-style sandboxes. WebDriver
            // still runs in Dispatch's test process sandbox, so this only disables Chrome's own
            // child-process sandbox layer.
            caps.add_arg("--no-sandbox")?;
            if run_chrome_single_process {
                // The Codex SDK workspace sandbox on macOS denies Mach service registration. In
                // Headless Shell, Chromium otherwise registers
                // org.chromium.Chromium.MachPortRendezvousServer.<pid> before DevTools startup for
                // child-process rendezvous. Keeping the headless browser in one process avoids that
                // bootstrap_check_in path; visible debugging runs stay multi-process.
                caps.add_arg("--single-process")?;
            }
            // Avoid /dev/shm startup failures in restricted environments by using regular temp
            // files for Chrome IPC/shared-memory storage.
            caps.add_arg("--disable-dev-shm-usage")?;
            Ok(())
        })
        .with_test_parallelism(BrowserTestParallelism::Sequential)
        .with_failure_policy(BrowserTestFailurePolicy::RunAll)
        .with_visibility(browser_visibility)
        .with_pause(PauseConfig::from_env())
        .with_timeouts(
            BrowserTimeouts::builder()
                .implicit_wait_timeout(Duration::from_secs(10))
                .page_load_timeout(Duration::from_secs(20))
                .build(),
        )
        .run(&app, BrowserTests::new().with(DispatchBoardTest))
        .await;

    run_result.map_err(Report::into_dynamic)?;

    Ok(())
}

struct DispatchTestApp {
    _app: LeptosTestApp,
    _tmpdir: TempDir,
    database: PathBuf,
    base_url: String,
}

impl DispatchTestApp {
    async fn start() -> Result<Self, Report> {
        let tmpdir =
            tempfile::tempdir().context("failed to create Dispatch browser-test temp dir")?;
        let database = tmpdir.path().join("dispatch.sqlite3");
        let editor_bin_dir = tmpdir.path().join("bin");
        fs::create_dir(&editor_bin_dir).context("failed to create browser-test editor bin dir")?;
        for program in [
            "rustrover",
            "rustrover64.exe",
            "code",
            "code.cmd",
            "code.exe",
        ] {
            write_test_executable(&editor_bin_dir.join(program))?;
        }
        let test_path = path_with_prefix(&editor_bin_dir)?;

        let app = LeptosTestAppConfig::new(env!("CARGO_MANIFEST_DIR"))
            .with_app_name("dispatch browser test")
            .with_forward_logs(true)
            .with_startup_line("Serving Dispatch")
            .with_env("DISPATCH_DATABASE", database.as_os_str())
            .with_env("PATH", test_path.as_os_str())
            .start()
            .await
            .map_err(Report::into_dynamic)?;

        let base_url = app.base_url().to_owned();

        Ok(Self {
            _app: app,
            _tmpdir: tmpdir,
            database,
            base_url,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

fn path_with_prefix(prefix: &Path) -> Result<std::ffi::OsString, Report> {
    let mut entries = vec![prefix.to_path_buf()];
    if let Some(path) = env::var_os("PATH") {
        entries.extend(env::split_paths(&path));
    }
    Ok(env::join_paths(entries).context("failed to build browser-test PATH")?)
}

fn write_test_executable(path: &Path) -> Result<(), Report> {
    fs::write(path, "#!/bin/sh\nexit 0\n").context("failed to write browser-test editor shim")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .context("failed to stat browser-test editor shim")?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .context("failed to make browser-test editor shim executable")?;
    }
    Ok(())
}

struct DispatchBoardTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for DispatchBoardTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("dispatch board renders and creates work")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        driver
            .goto(app.url("/projects"))
            .await
            .context("failed to open Dispatch projects page")?;

        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Projects");
        find(driver, By::Css(".project-switcher")).await?;
        find(driver, By::Css("[data-crudkit-leptos='projects']")).await?;
        assert_source_contains(driver, "project-switcher").await?;
        assert_source_does_not_contain(driver, ">Switch<").await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"projects\"").await?;
        assert_source_does_not_contain(driver, "Existing projects").await?;
        assert_source_does_not_contain(driver, "project-create-form").await?;
        assert_source_contains(driver, "Codex app-server").await?;
        find(driver, By::Css(".topbar-codex")).await?;
        assert_source_does_not_contain(driver, "codex-status-panel").await?;
        click(driver, By::Css(".topbar-codex")).await?;
        assert_that!(
            driver
                .title()
                .await
                .context("failed to read Codex page title")?
        )
        .is_equal_to("Codex automation");
        find(driver, By::Css(".codex-status-panel")).await?;
        assert_codex_auth_guide_when_blocked(driver).await?;
        driver
            .goto(app.url("/projects"))
            .await
            .context("failed to reopen Dispatch projects page after Codex status check")?;
        assert_source_contains(driver, "data-crudkit-leptos=\"agent-tools\"").await?;
        assert_source_does_not_contain(driver, "/agent-tools/create").await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
        )
        .await?;
        click(
            driver,
            By::Css("[data-crudkit-leptos='projects'] .crud-nav button"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] select.agent-model-select"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] select.agent-reasoning-select"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] option[value='gpt-5.6-sol']"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] option[value='max']"),
        )
        .await?;
        driver
            .goto(app.url("/projects"))
            .await
            .context("failed to reopen Dispatch projects page after create-view check")?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='agent-tools'] .crud-nav"),
        )
        .await?;
        assert_source_does_not_contain(driver, "Invalid URL").await?;
        assert_source_does_not_contain(driver, "relative URL without a base").await?;

        create_project(driver).await?;
        create_alternate_project(driver).await?;
        seed_system_prompt_history(driver).await?;
        seed_memory_history(driver).await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to open Dispatch board page")?;

        find(driver, By::Css("section.project-settings")).await?;
        find(driver, By::Css("section.board")).await?;
        assert_board_shell_uses_viewport_width(driver).await?;
        assert_board_layout_is_lane_first(driver).await?;
        find(driver, By::Css(".workspace-bar > .workspace-actions")).await?;
        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Dispatch");
        assert_source_contains(driver, "Copy path").await?;
        assert_source_does_not_contain(driver, "Copy cd").await?;
        assert_source_contains(driver, "Open folder").await?;
        assert_source_contains(driver, "Open RustRover").await?;
        assert_source_contains(driver, "Open VS Code").await?;
        find(
            driver,
            By::Css("img.workspace-button-icon[src=\"/icons/workspace-rustrover.svg\"]"),
        )
        .await?;
        find(
            driver,
            By::Css("img.workspace-button-icon[src=\"/icons/workspace-vscode.svg\"]"),
        )
        .await?;
        assert_source_contains(driver, "Git repository").await?;
        find(driver, By::Css(".workspace-git-status")).await?;
        find(driver, By::Css(".workspace-git-diff")).await?;
        assert_source_does_not_contain(driver, "Open IDE").await?;
        assert_source_contains(driver, "System prompt").await?;
        assert_source_does_not_contain(driver, "project-option-key").await?;
        assert_source_contains(driver, "Memory").await?;
        assert_source_contains(driver, "Automation policy").await?;
        assert_source_contains(driver, "Read-only agents").await?;
        find(driver, By::Css("#project-max-read-only-agents")).await?;
        assert_source_contains(driver, "Auto-Commit").await?;
        find(driver, By::Css("#project-auto-commit")).await?;
        find(driver, By::Css("#project-commit-standard")).await?;
        find(
            driver,
            By::Css("#project-revert-strategy option[value='git_reset']"),
        )
        .await?;
        assert_source_contains(driver, "system prompt history").await?;
        assert_source_contains(driver, "memory history").await?;
        assert_source_does_not_contain(driver, "Compact history").await?;
        assert_source_does_not_contain(driver, "Append memory").await?;
        assert_source_does_not_contain(driver, "append-memory").await?;
        assert_source_does_not_contain(driver, "/memory/append").await?;
        assert_source_does_not_contain(driver, "memory-history-entry").await?;
        assert_source_does_not_contain(driver, "memory-snapshot").await?;
        assert_source_does_not_contain(driver, "Allow refinement while editing").await?;
        assert_settings_response_omits_refinement_policy(driver).await?;
        find(driver, By::Css("#project-system-prompt-version")).await?;
        find(driver, By::Css("textarea.project-system-prompt-text")).await?;
        assert_system_prompt_history_selector_behaviour(driver).await?;
        find(driver, By::Css("#project-memory-version")).await?;
        find(driver, By::Css("textarea.project-memory-text")).await?;
        assert_memory_history_selector_behaviour(driver).await?;
        assert_source_does_not_contain(driver, "Run settings").await?;
        assert_top_nav_order(driver).await?;
        find(driver, By::Css(".top-nav a[href='/runs?project=demo']")).await?;
        assert_source_does_not_contain(driver, "No runs yet").await?;
        assert_source_does_not_contain(driver, "CrudKit resources").await?;
        find(driver, By::Css(".topbar-codex")).await?;
        assert_source_does_not_contain(driver, "codex-status-panel").await?;
        find(driver, By::Css(".topbar-auto-commit[role='switch']")).await?;
        assert_auto_commit_toggle_updates_without_navigation(driver).await?;
        find(driver, By::Css(".topbar-automation button")).await?;
        assert_source_contains(driver, "Stopped").await?;
        assert_source_does_not_contain(driver, "Start automation").await?;
        assert_source_does_not_contain(driver, "Recover stale claims").await?;
        assert_source_contains(driver, "Maintenance").await?;
        assert_source_contains(driver, "Cleanup worktrees").await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"work-items\"").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"swim-lanes\"").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"work-item-states\"").await?;
        assert_source_does_not_contain(driver, "Deserialize(").await?;
        assert_source_does_not_contain(driver, "missing field `identifier`").await?;
        assert_source_does_not_contain(driver, "unknown variant `Position`").await?;
        find(driver, By::Css(".lane:nth-child(1) .lane-edit")).await?;
        find(driver, By::Css(".lane:nth-child(1) .lane-header .lane-add")).await?;
        find(driver, By::Css(".lane:nth-child(2) .lane-add")).await?;
        assert_lane_add_button_count(driver, 2).await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"automation-triggers\"")
            .await?;
        assert_frontend_route_navigation_renders_page(driver).await?;
        assert_service_get_results_use_local_storage(driver).await?;
        assert_crudkit_create_form_survives_live_event(driver).await?;
        assert_request_error_toast_preserves_page_and_draft(driver).await?;

        driver
            .goto(app.url("/projects?project=demo"))
            .await
            .context("failed to open Dispatch projects page for workflow authoring")?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='work-item-states'] .crud-nav"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='swim-lanes'] .crud-nav"),
        )
        .await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"work-item-states\"").await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"swim-lanes\"").await?;
        assert_swim_lane_create_form_exposes_structured_filter(driver).await?;
        let structured_lane_id = create_swim_lane_filter_seed_lane(driver).await?;
        edit_swim_lane_filter_through_structured_controls(driver, app, structured_lane_id).await?;
        create_structured_lane_matching_item(driver).await?;
        assert_structured_swim_lane_filter_board_behaviour(driver, app).await?;

        driver
            .goto(app.url("/runs?project=demo"))
            .await
            .context("failed to open Dispatch runs page")?;
        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Runs");
        find(driver, By::Css(".runs-page .automation")).await?;
        find(
            driver,
            By::Css(".top-nav a.active[href='/runs?project=demo']"),
        )
        .await?;
        assert_source_contains(driver, "No runs yet").await?;
        assert_source_contains(driver, "0 running (0 mutating, 0 read-only)").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"automation-triggers\"")
            .await?;
        seed_run_commit_outcome_fixtures(app).await?;
        assert_run_log_commit_fixture(
            driver,
            app,
            501,
            "status-completed",
            "Done. Created browser-test commit fixture.",
            "committed 0123456789ab (required)",
        )
        .await?;
        assert_run_output_fixture(driver, app).await?;
        assert_run_log_commit_fixture(
            driver,
            app,
            502,
            "status-failed",
            "Missing required commit: completed run left uncommitted changes.",
            "missing required commit (required)",
        )
        .await?;

        driver
            .goto(app.url("/automation?project=demo"))
            .await
            .context("failed to open Dispatch automation page")?;
        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Automation");
        find(
            driver,
            By::Css("[data-crudkit-leptos='automation-triggers'] .crud-nav"),
        )
        .await?;
        find(driver, By::Css(".trigger-runs")).await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"automation-triggers\"").await?;
        assert_source_contains(driver, "Work-consuming automations").await?;
        assert_source_contains(driver, "Work-producing automations").await?;
        assert_source_contains(driver, "Mutability").await?;
        assert_source_contains(driver, "No automation selected").await?;
        assert_source_does_not_contain(driver, "Create trigger").await?;
        assert_source_does_not_contain(driver, "trigger-edit-form").await?;

        create_trigger(driver).await?;
        driver
            .goto(app.url("/automation?project=demo"))
            .await
            .context("failed to reload Dispatch automation page after automation creation")?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='automation-triggers'] .crud-nav"),
        )
        .await?;
        find(driver, By::XPath("//*[contains(text(), 'refine-new')]")).await?;
        assert_source_contains(driver, "refine-new").await?;

        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to reopen Dispatch board page")?;
        assert_source_does_not_contain(driver, "Dispatch labels").await?;
        driver
            .goto(app.url("/api/docs?project=demo"))
            .await
            .context("failed to open Dispatch API page")?;
        find(driver, By::Css("section.dispatch-labels")).await?;
        assert_source_contains(driver, "dispatch:automation-blocked").await?;
        assert_source_contains(driver, "dispatch:feedback-requested").await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to reopen Dispatch board page after API check")?;
        open_new_item_modal(driver).await?;
        assert_new_item_modal_actions(driver).await?;
        find(driver, By::Css("#new-item-modal select[name='state']")).await?;
        assert_lane_new_item_state(driver).await?;
        close_clean_new_item_modal(driver).await?;
        assert_lane_add_preselects_state(driver).await?;
        assert_new_item_modal_dirty_leave_protection(driver).await?;
        open_new_item_modal(driver).await?;
        find(driver, By::Css("#new-item-modal select.agent-model-select")).await?;
        assert_source_contains(driver, "Project default").await?;
        set_input_value(driver, "#new-item-modal .crud-input-field", "Browser item").await?;
        set_input_value(
            driver,
            "#new-item-modal input[name='description']",
            "Created through browser-test\nSecond line",
        )
        .await?;
        append_new_item_initial_label(driver, "area", "browser").await?;
        append_new_item_initial_label(driver, "needs-verification", "").await?;
        click_new_item_save(driver).await?;

        find(driver, By::LinkText("Browser item")).await?;
        assert_board_card_contains(driver, "Browser item", "area=browser").await?;
        assert_board_card_contains(driver, "Browser item", "needs-verification").await?;
        assert_source_contains(driver, "Created through browser-test").await?;
        assert_source_contains(driver, "state=idea").await?;

        click(driver, By::LinkText("Browser item")).await?;
        find(driver, By::Css("section.item-settings")).await?;
        find(driver, By::Css("section.comments")).await?;
        assert_source_contains(driver, "Item details").await?;
        assert_source_contains(driver, "area=browser").await?;
        assert_source_contains(driver, "needs-verification").await?;
        assert_item_detail_description_is_not_duplicated(driver).await?;
        assert_item_detail_description_editor_accepts_click_and_text(driver).await?;
        let relationship_target_id = create_relationship_target_item(driver).await?;
        assert_item_relationship_create_delete_flow(driver, relationship_target_id).await?;
        assert_item_detail_dirty_leave_protection(driver).await?;
        click(driver, By::LinkText("Browser item")).await?;
        find(driver, By::Css("section.item-settings")).await?;
        assert_source_does_not_contain(driver, "automation can claim this item").await?;
        assert_source_does_not_contain(driver, "Set state").await?;
        find(
            driver,
            By::XPath(
                "//section[contains(@class, 'item-settings')]//button[contains(., 'Löschen')]",
            ),
        )
        .await?;
        assert_source_does_not_contain(driver, "Start agent").await?;
        assert_source_contains(driver, "Comments").await?;
        assert_user_comment_create_flow(driver).await?;
        add_agent_comment(driver).await?;
        claim_current_item(driver).await?;
        let item_url = driver
            .current_url()
            .await
            .context("failed to read item URL after adding agent comment")?;
        driver
            .goto(item_url.as_str())
            .await
            .context("failed to reload item page after adding agent comment")?;
        find(
            driver,
            By::Css(
                "section.comments .comment-author-link[href='/projects/demo/automation/runs/60/log']",
            ),
        )
        .await?;
        find(
            driver,
            By::Css(".item-meta a.claim-badge[href='/projects/demo/automation/runs/60/log']"),
        )
        .await?;
        assert_source_contains(driver, "dispatch-run-60").await?;
        find(driver, By::Css("section.item-labels")).await?;
        assert_state_label_dropdown_and_move(driver).await?;
        send_keys(
            driver,
            By::Css(".label-add-form input[name='key']"),
            "severity",
        )
        .await?;
        send_keys(
            driver,
            By::Css(".label-add-form input[name='value']"),
            "high",
        )
        .await?;
        submit_label_add_form(driver).await?;
        find(driver, By::XPath("//*[contains(text(), 'severity=high')]")).await?;
        assert_label_add_preserved_item_page(driver).await?;
        assert_item_label_update_delete_flow(driver).await?;

        driver
            .goto(app.url("/projects?project=demo"))
            .await
            .context("failed to reopen Dispatch projects page")?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
        )
        .await?;
        find(
            driver,
            By::XPath("//*[@data-crudkit-leptos='projects']//*[normalize-space()='Demo']"),
        )
        .await?;
        assert_source_contains(driver, "Demo").await?;
        assert_source_does_not_contain(driver, "project-edit-form").await?;

        Ok(())
    }
}

async fn create_project(driver: &WebDriver) -> Result<(), Report> {
    let created = driver
        .execute_async(
            r#"
            const done = arguments[0];
            fetch('/projects', {
                method: 'POST',
                headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                body: new URLSearchParams({
                    name: 'demo',
                    display_name: 'Demo',
                    path: '.',
                }),
            }).then(response => done(response.ok)).catch(() => done(false));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to create project through browser-test setup request")?
        .convert::<bool>()
        .context("failed to read project setup response")?;
    assert_that!(created).is_true();
    Ok(())
}

async fn create_alternate_project(driver: &WebDriver) -> Result<(), Report> {
    let created = driver
        .execute_async(
            r#"
            const done = arguments[0];
            fetch('/projects', {
                method: 'POST',
                headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                body: new URLSearchParams({
                    name: 'demo-alt',
                    display_name: 'Demo Alt',
                    path: '.',
                }),
            }).then(response => done(response.ok)).catch(() => done(false));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to create alternate project through browser-test setup request")?
        .convert::<bool>()
        .context("failed to read alternate project setup response")?;
    assert_that!(created).is_true();
    Ok(())
}

async fn seed_run_commit_outcome_fixtures(app: &DispatchTestApp) -> Result<(), Report> {
    let db = Database::connect(format!("sqlite://{}?mode=rwc", app.database.display()))
        .await
        .context("failed to connect to Dispatch browser-test database")?;
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA foreign_keys = ON".to_owned(),
    ))
    .await
    .context("failed to enable browser-test SQLite foreign keys")?;

    // These run fields are server-owned and intentionally excluded from public CrudKit create and
    // update models, so the browser test seeds deterministic rows directly in its private database.
    let committed = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            INSERT INTO "agent_runs" (
                "id",
                "project_id",
                "work_item_id",
                "memory_event_id",
                "trigger_id",
                "trigger_name",
                "tool_name",
                "mutability",
                "status",
                "command",
                "working_dir",
                "worktree_path",
                "branch_name",
                "process_id",
                "exit_code",
                "log_path",
                "developer_instructions_path",
                "user_prompt_path",
                "agent_model",
                "agent_reasoning_effort",
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "commit_required",
                "commit_outcome",
                "commit_shas",
                "pr_requested",
                "pr_url",
                "cleanup_status",
                "worktree_cleaned_at",
                "result_summary",
                "started_at",
                "finished_at",
                "created_at",
                "updated_at"
            )
            SELECT
                501,
                "id",
                NULL,
                NULL,
                NULL,
                NULL,
                'codex',
                'mutating',
                'completed',
                'codex --browser-test',
                '/tmp/dispatch-browser-test',
                NULL,
                NULL,
                NULL,
                0,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                1,
                'committed',
                '["0123456789abcdef0123456789abcdef01234567"]',
                0,
                NULL,
                'not_applicable',
                NULL,
                'Done. Created browser-test commit fixture.',
                '2026-06-19T10:00:00Z',
                '2026-06-19T10:01:00Z',
                '2026-06-19T10:00:00Z',
                '2026-06-19T10:01:00Z'
            FROM "projects"
            WHERE "name" = 'demo';
            "#
            .to_owned(),
        ))
        .await
        .context("failed to seed committed automation run fixture")?;
    assert_that!(committed.rows_affected()).is_equal_to(1);

    let missing_required = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            INSERT INTO "agent_runs" (
                "id",
                "project_id",
                "work_item_id",
                "memory_event_id",
                "trigger_id",
                "trigger_name",
                "tool_name",
                "mutability",
                "status",
                "command",
                "working_dir",
                "worktree_path",
                "branch_name",
                "process_id",
                "exit_code",
                "log_path",
                "developer_instructions_path",
                "user_prompt_path",
                "agent_model",
                "agent_reasoning_effort",
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "commit_required",
                "commit_outcome",
                "commit_shas",
                "pr_requested",
                "pr_url",
                "cleanup_status",
                "worktree_cleaned_at",
                "result_summary",
                "started_at",
                "finished_at",
                "created_at",
                "updated_at"
            )
            SELECT
                502,
                "id",
                NULL,
                NULL,
                NULL,
                NULL,
                'codex',
                'mutating',
                'failed',
                'codex --browser-test',
                '/tmp/dispatch-browser-test',
                NULL,
                NULL,
                NULL,
                0,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                1,
                'missing_required',
                '[]',
                0,
                NULL,
                'not_applicable',
                NULL,
                'Missing required commit: completed run left uncommitted changes.',
                '2026-06-19T10:02:00Z',
                '2026-06-19T10:03:00Z',
                '2026-06-19T10:02:00Z',
                '2026-06-19T10:03:00Z'
            FROM "projects"
            WHERE "name" = 'demo';
            "#
            .to_owned(),
        ))
        .await
        .context("failed to seed missing-required automation run fixture")?;
    assert_that!(missing_required.rows_affected()).is_equal_to(1);

    let output_path = app._tmpdir.path().join("run-503.output.json");
    let command = r#"/bin/zsh -lc "sed -n '1,4p' design/ui.md""#;
    let generic_command = "/bin/zsh -lc 'just check'";
    let diff_command = "/bin/zsh -lc 'git diff -- design/ui.md'";
    let diff_output = "diff --git a/design/ui.md b/design/ui.md\nindex 1111111..2222222 100644\n--- a/design/ui.md\n+++ b/design/ui.md\n@@ -1 +1 @@\n-old copy\n+new copy";
    let output = serde_json::json!({
        "schema_version": 1,
        "pieces": [
            {
                "sequence": 1,
                "timestamp": "2026-06-19T10:04:00Z",
                "kind": "reasoning",
                "source": "codex",
                "item_id": "reasoning_1",
                "title": "thinking",
                "body": "",
                "metadata": { "status": "started" }
            },
            {
                "sequence": 2,
                "timestamp": "2026-06-19T10:04:08Z",
                "kind": "reasoning",
                "source": "codex",
                "item_id": "reasoning_1",
                "title": "thinking",
                "body": "",
                "metadata": { "status": "completed" }
            },
            {
                "sequence": 3,
                "timestamp": "2026-06-19T10:04:09Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_1",
                "title": "command started",
                "body": command,
                "metadata": {
                    "tool_type": "command",
                    "status": "started",
                    "command": command
                }
            },
            {
                "sequence": 4,
                "timestamp": "2026-06-19T10:04:10Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_1",
                "title": "command completed",
                "body": command,
                "metadata": {
                    "tool_type": "command",
                    "status": "completed",
                    "command": command,
                    "exit_code": 0,
                    "output": "line one\nline two\nline three\nline four"
                }
            },
            {
                "sequence": 5,
                "timestamp": "2026-06-19T10:04:11Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_2",
                "title": "command started",
                "body": generic_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "started",
                    "command": generic_command
                }
            },
            {
                "sequence": 6,
                "timestamp": "2026-06-19T10:04:12Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_2",
                "title": "command completed",
                "body": generic_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "completed",
                    "command": generic_command,
                    "exit_code": 0,
                    "output": "checked"
                }
            },
            {
                "sequence": 7,
                "timestamp": "2026-06-19T10:04:13Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_3",
                "title": "command started",
                "body": diff_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "started",
                    "command": diff_command
                }
            },
            {
                "sequence": 8,
                "timestamp": "2026-06-19T10:04:14Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_3",
                "title": "command completed",
                "body": diff_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "completed",
                    "command": diff_command,
                    "exit_code": 0,
                    "output": diff_output
                }
            },
            {
                "sequence": 9,
                "timestamp": "2026-06-19T10:04:15Z",
                "kind": "model_message",
                "source": "codex",
                "item_id": "message_1",
                "title": "final answer",
                "body": "Readable model output.",
                "metadata": { "final_answer": true }
            }
        ]
    });
    fs::write(
        &output_path,
        serde_json::to_vec_pretty(&output).context("failed to encode run-output fixture")?,
    )
    .context("failed to write run-output fixture")?;
    let compact_output = db
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            INSERT INTO "agent_runs" (
                "id",
                "project_id",
                "work_item_id",
                "memory_event_id",
                "trigger_id",
                "trigger_name",
                "tool_name",
                "mutability",
                "status",
                "command",
                "working_dir",
                "worktree_path",
                "branch_name",
                "process_id",
                "exit_code",
                "log_path",
                "developer_instructions_path",
                "user_prompt_path",
                "agent_model",
                "agent_reasoning_effort",
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "commit_required",
                "commit_outcome",
                "commit_shas",
                "pr_requested",
                "pr_url",
                "cleanup_status",
                "worktree_cleaned_at",
                "result_summary",
                "started_at",
                "finished_at",
                "created_at",
                "updated_at"
            )
            SELECT
                503,
                "id",
                NULL,
                NULL,
                NULL,
                NULL,
                'codex',
                'mutating',
                'completed',
                'codex --browser-test',
                '/tmp/dispatch-browser-test',
                NULL,
                NULL,
                NULL,
                0,
                ?1,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                0,
                'not_required',
                '[]',
                0,
                NULL,
                'not_applicable',
                NULL,
                'Rendered compact browser-test output.',
                '2026-06-19T10:04:00Z',
                '2026-06-19T10:04:13Z',
                '2026-06-19T10:04:00Z',
                '2026-06-19T10:04:13Z'
            FROM "projects"
            WHERE "name" = 'demo';
            "#,
            vec![output_path.to_string_lossy().into_owned().into()],
        ))
        .await
        .context("failed to seed compact run-output fixture")?;
    assert_that!(compact_output.rows_affected()).is_equal_to(1);

    Ok(())
}

async fn assert_run_output_fixture(
    driver: &WebDriver,
    app: &DispatchTestApp,
) -> Result<(), Report> {
    driver
        .goto(app.url("/projects/demo/automation/runs/503/log"))
        .await
        .context("failed to open compact run-output fixture")?;
    let toggle = find(driver, By::Css(".thinking-history-toggle")).await?;
    assert_that!(
        toggle
            .text()
            .await
            .context("failed to read thinking toggle")?
    )
    .is_equal_to("Thinking (1)");
    assert_that!(
        driver
            .execute(
                "return document.querySelectorAll('.output-reasoning-history').length;",
                Vec::new(),
            )
            .await
            .context("failed to inspect hidden thinking history")?
            .convert::<i64>()
            .context("failed to read hidden thinking history count")?
    )
    .is_equal_to(0);

    let exploring = find(driver, By::Css(".command-exploring")).await?;
    assert_that!(
        exploring
            .text()
            .await
            .context("failed to read exploring summary")?
    )
    .is_equal_to("Exploring design/ui.md...");
    find(
        driver,
        By::XPath("//code[normalize-space()='Ran just check']"),
    )
    .await?;
    assert_source_does_not_contain(driver, "exit 0").await?;
    assert_source_does_not_contain(driver, "Command details").await?;
    assert_source_does_not_contain(driver, "Hide full output").await?;

    let preview = find(driver, By::Css(".tool-output-preview")).await?;
    assert_that!(
        preview
            .text()
            .await
            .context("failed to read compact output preview")?
    )
    .is_equal_to("line one\nline two");
    click(driver, By::Css(".tool-output-block summary")).await?;
    assert_that!(
        find(
            driver,
            By::Css(".tool-output-block[open] .tool-output-full")
        )
        .await?
        .text()
        .await
        .context("failed to read expanded command output")?
    )
    .is_equal_to("line one\nline two\nline three\nline four");
    click(
        driver,
        By::Css(".tool-output-block[open] .tool-output-full"),
    )
    .await?;
    assert_that!(
        driver
            .execute(
                "return document.querySelector('.tool-output-block')?.open ?? true;",
                Vec::new(),
            )
            .await
            .context("failed to inspect collapsed command output")?
            .convert::<bool>()
            .context("failed to read collapsed command output state")?
    )
    .is_false();

    find(
        driver,
        By::XPath("//code[normalize-space()='Ran git diff -- design/ui.md']"),
    )
    .await?;
    let collapsed_diff_visibility = driver
        .execute(
            r#"
            const row = [...document.querySelectorAll('.output-command')]
                .find(candidate => candidate.querySelector('.command-summary')?.textContent.trim() === 'Ran git diff -- design/ui.md');
            const details = row?.querySelector('.tool-output-block');
            const preview = details?.querySelector('.tool-output-preview');
            const full = details?.querySelector('.tool-output-full');
            return details && preview && full
                ? `${details.open}|${getComputedStyle(preview).display}|${getComputedStyle(full).display}`
                : '';
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect collapsed diff output")?
        .convert::<String>()
        .context("failed to read collapsed diff visibility")?;
    assert_that!(collapsed_diff_visibility).is_equal_to("false|block|none");
    click(
        driver,
        By::XPath(
            "//article[.//code[normalize-space()='Ran git diff -- design/ui.md']]//details[contains(@class, 'tool-output-block')]/summary",
        ),
    )
    .await?;
    let expanded_diff_visibility = driver
        .execute(
            r#"
            const row = [...document.querySelectorAll('.output-command')]
                .find(candidate => candidate.querySelector('.command-summary')?.textContent.trim() === 'Ran git diff -- design/ui.md');
            const details = row?.querySelector('.tool-output-block');
            const preview = details?.querySelector('.tool-output-preview');
            const full = details?.querySelector('.tool-output-full');
            return details && preview && full
                ? `${details.open}|${getComputedStyle(preview).display}|${getComputedStyle(full).display}`
                : '';
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect expanded diff output")?
        .convert::<String>()
        .context("failed to read expanded diff visibility")?;
    assert_that!(expanded_diff_visibility).is_equal_to("true|none|block");

    click(driver, By::Css(".thinking-history-toggle input")).await?;
    let history = find(driver, By::Css(".output-reasoning-history")).await?;
    assert_that!(
        history
            .text()
            .await
            .context("failed to read thinking history row")?
    )
    .is_equal_to("Thought for 8s");
    assert_that!(
        driver
            .execute(
                "return document.querySelectorAll('.output-reasoning-history details').length;",
                Vec::new(),
            )
            .await
            .context("failed to inspect empty thinking disclosures")?
            .convert::<i64>()
            .context("failed to read empty thinking disclosure count")?
    )
    .is_equal_to(0);

    let colors = driver
        .execute(
            r#"
            const output = document.querySelector('.model-output');
            const section = output?.closest('section');
            const toolOutput = document.querySelector('.tool-output-preview');
            return output && section && toolOutput
                ? `${getComputedStyle(output).color}|${getComputedStyle(section).backgroundColor}|${getComputedStyle(toolOutput).color}`
                : '';
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect run-output colors")?
        .convert::<String>()
        .context("failed to read run-output colors")?;
    assert_that!(colors).is_equal_to("rgb(32, 36, 42)|rgb(255, 255, 255)|rgb(104, 116, 130)");

    Ok(())
}

async fn assert_run_log_commit_fixture(
    driver: &WebDriver,
    app: &DispatchTestApp,
    run_id: i64,
    expected_result_class: &str,
    expected_summary: &str,
    expected_commit: &str,
) -> Result<(), Report> {
    driver
        .goto(app.url(&format!("/projects/demo/automation/runs/{run_id}/log")))
        .await
        .context_with(|| format!("failed to open run #{run_id} log page"))?;
    find(
        driver,
        By::XPath(format!(
            "//main[contains(@class, 'run-log')]//h1[normalize-space()='Run #{run_id}']"
        )),
    )
    .await?;
    find(
        driver,
        By::XPath(
            "//main[contains(@class, 'run-log')]//h2[normalize-space()='Developer instructions']",
        ),
    )
    .await?;
    find(
        driver,
        By::XPath("//main[contains(@class, 'run-log')]//h2[normalize-space()='User prompt']"),
    )
    .await?;

    let summary = run_log_detail_text(driver, "result").await?;
    assert_that!(summary).is_equal_to(expected_summary.to_owned());
    let result_class = driver
        .execute(
            r#"
            return document
                .querySelector('main.run-log .run-result-inline')
                ?.className
                ?? '';
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect run result class")?
        .convert::<String>()
        .context("failed to read run result class")?;
    assert_that!(result_class).contains(expected_result_class);

    let commit = run_log_detail_text(driver, "commit").await?;
    assert_that!(commit).is_equal_to(expected_commit.to_owned());
    Ok(())
}

async fn run_log_detail_text(driver: &WebDriver, term: &str) -> Result<String, Report> {
    let script = format!(
        r#"
        const term = {term:?};
        const dt = Array.from(document.querySelectorAll('main.run-log dt'))
            .find((element) => element.textContent.trim() === term);
        const dd = dt?.nextElementSibling;
        return dd?.tagName === 'DD' ? dd.textContent.trim() : '';
        "#
    );
    let value = driver
        .execute(&script, Vec::new())
        .await
        .context_with(|| format!("failed to inspect run-log detail {term:?}"))?
        .convert::<String>()
        .context_with(|| format!("failed to read run-log detail {term:?}"))?;
    if value.is_empty() {
        bail!("missing run-log detail {term:?}");
    }
    Ok(value)
}

async fn assert_swim_lane_create_form_exposes_structured_filter(
    driver: &WebDriver,
) -> Result<(), Report> {
    click(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] .crud-nav button"),
    )
    .await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] [data-lane-filter-editor='structured']"),
    )
    .await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] [data-lane-filter-add-clause='true']"),
    )
    .await?;
    assert_source_does_not_contain(driver, "placeholder=\"{&quot;All&quot;").await?;
    Ok(())
}

async fn create_swim_lane_filter_seed_lane(driver: &WebDriver) -> Result<i64, Report> {
    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            fetch('/api/swim_lanes/crud/create-one', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    entity: {
                        project_id: 1,
                        identifier: 'filtered',
                        name: 'Filtered',
                        position: 45,
                        filter: '{"All":[]}',
                        item_order: 'updated_desc',
                        can_create_items: true
                    }
                }),
            }).then(async response => {
                if (!response.ok) {
                    done(await response.text());
                    return;
                }
                const saved = await response.json();
                done(String(saved?.entity?.id ?? 'missing id'));
            }).catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to create swim-lane through CrudKit browser-test setup request")?
        .convert::<String>()
        .context("failed to read swim-lane setup response")?;
    Ok(result
        .parse::<i64>()
        .context_with(|| format!("failed to parse created swim-lane id from {result:?}"))?)
}

async fn edit_swim_lane_filter_through_structured_controls(
    driver: &WebDriver,
    app: &DispatchTestApp,
    lane_id: i64,
) -> Result<(), Report> {
    driver
        .goto(app.url(&format!(
            "/projects?project=demo&edit_swim_lane={lane_id}#swim-lanes"
        )))
        .await
        .context("failed to open swim-lane edit form")?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] [data-lane-filter-editor='structured']"),
    )
    .await?;

    let script = r#"
        const done = arguments[0];
        const frame = () => new Promise((resolve) => requestAnimationFrame(() => resolve()));
        const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
        const waitFor = async (predicate, label) => {
            const deadline = Date.now() + 5000;
            while (Date.now() <= deadline) {
                const value = predicate();
                if (value) {
                    return value;
                }
                await sleep(50);
            }
            throw new Error(`missing ${label}`);
        };
        const fire = (element, eventName) => {
            element.dispatchEvent(new Event(eventName, { bubbles: true }));
        };
        const setInput = (element, value) => {
            if (!element) {
                throw new Error('missing input');
            }
            element.value = value;
            element.setAttribute('value', value);
            fire(element, 'input');
            fire(element, 'change');
        };
        const setSelect = (element, value) => {
            if (!element) {
                throw new Error('missing select');
            }
            element.value = value;
            fire(element, 'change');
        };
        const clickAndFrame = async (root, selector) => {
            const button = root?.querySelector(selector);
            if (!button) {
                throw new Error(`missing button ${selector}`);
            }
            button.click();
            await frame();
            await frame();
        };
        const directClauses = (group) =>
            Array.from(group.querySelectorAll(':scope > .lane-filter-elements > .lane-filter-clause'));

        (async () => {
            const panel = await waitFor(
                () => document.querySelector("[data-crudkit-leptos='swim-lanes']"),
                'swim-lanes panel',
            );
            const editor = await waitFor(
                () => panel.querySelector("[data-lane-filter-editor='structured']"),
                'filter editor',
            );
            const rootGroup = await waitFor(
                () => editor.querySelector("[data-lane-filter-group='root']"),
                'root filter group',
            );

            await clickAndFrame(rootGroup, "[data-lane-filter-add-clause='true']");
            let rootClauses = directClauses(rootGroup);
            setInput(rootClauses[0]?.querySelector("[data-lane-filter-key='true']"), 'state');
            setInput(rootClauses[0]?.querySelector("[data-lane-filter-value='true']"), 'open');

            await clickAndFrame(rootGroup, "[data-lane-filter-add-group='true']");
            const nestedGroup = await waitFor(
                () => editor.querySelector("[data-lane-filter-group='1']"),
                'nested filter group',
            );
            setSelect(nestedGroup.querySelector('.lane-filter-group-kind select'), 'any');

            await clickAndFrame(nestedGroup, "[data-lane-filter-add-clause='true']");
            let nestedClauses = directClauses(nestedGroup);
            setInput(nestedClauses[0]?.querySelector("[data-lane-filter-key='true']"), 'severity');
            setSelect(nestedClauses[0]?.querySelector("[data-lane-filter-operator='true']"), 'is_in');
            await frame();
            nestedClauses = directClauses(nestedGroup);
            setInput(
                nestedClauses[0]?.querySelector("[data-lane-filter-value-list='true']"),
                'critical, high',
            );

            await clickAndFrame(nestedGroup, "[data-lane-filter-add-clause='true']");
            nestedClauses = directClauses(nestedGroup);
            setInput(
                nestedClauses[1]?.querySelector("[data-lane-filter-key='true']"),
                'needs-verification',
            );
            setSelect(
                nestedClauses[1]?.querySelector("[data-lane-filter-operator='true']"),
                'present',
            );
            await frame();

            const rawToggle = editor.querySelector('.lane-filter-raw-toggle');
            rawToggle.click();
            await frame();
            const raw = editor.querySelector("[data-lane-filter-raw='true']")?.value ?? '';
            if (!raw.includes('"Any"') || !raw.includes('"severity"') || !raw.includes('"is_in"')) {
                done(`unexpected raw filter ${raw}`);
                return;
            }
            editor.querySelector('.lane-filter-structured-toggle')?.click();
            await frame();
            done('ok');
        })().catch(error => done(String(error)));
        "#;
    let result = driver
        .execute_async(script, Vec::new())
        .await
        .context("failed to edit swim-lane filter through structured controls")?
        .convert::<String>()
        .context("failed to read structured swim-lane edit result")?;
    assert_that!(result).is_equal_to("ok".to_owned());
    click(
        driver,
        By::XPath(
            "//div[@data-crudkit-leptos='swim-lanes']//button[normalize-space()='Speichern']",
        ),
    )
    .await?;
    wait_for_structured_swim_lane_filter_saved(driver, lane_id).await?;
    Ok(())
}

async fn wait_for_structured_swim_lane_filter_saved(
    driver: &WebDriver,
    lane_id: i64,
) -> Result<(), Report> {
    let script = r#"
        const done = arguments[0];
        const laneId = LANE_ID;
        const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
        (async () => {
            const deadline = Date.now() + 5000;
            let lastFilter = '<not read>';
            while (Date.now() <= deadline) {
                const response = await fetch('/api/swim_lanes/crud/read-many', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        limit: 1,
                        skip: null,
                        order_by: null,
                        condition: {
                            All: [{
                                column_name: 'id',
                                operator: '=',
                                value: { I64: laneId }
                            }]
                        }
                    }),
                });
                if (response.ok) {
                    const rows = await response.json();
                    const lane = Array.isArray(rows) ? rows[0] : undefined;
                    const filter = lane?.filter ?? '';
                    lastFilter = filter || JSON.stringify(rows);
                    if (
                        filter.includes('"Any"')
                        && filter.includes('"state"')
                        && filter.includes('"severity"')
                        && filter.includes('"needs-verification"')
                    ) {
                        done('ok');
                        return;
                    }
                }
                await sleep(100);
            }
            done(`saved filter was not visible through CrudKit read-many; last=${lastFilter}`);
        })().catch(error => done(String(error)));
        "#
    .replace("LANE_ID", &lane_id.to_string());
    let result = driver
        .execute_async(script, Vec::new())
        .await
        .context("failed to verify saved structured swim-lane filter")?
        .convert::<String>()
        .context("failed to read saved structured swim-lane filter result")?;
    assert_that!(result).is_equal_to("ok".to_owned());
    Ok(())
}

async fn create_structured_lane_matching_item(driver: &WebDriver) -> Result<(), Report> {
    let created = driver
        .execute_async(
            r#"
            const done = arguments[0];
            fetch('/api/projects/demo/items', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    title: 'Structured lane item',
                    description: 'Created for the structured swim-lane filter browser test',
                    state: 'open',
                    initial_labels: [{ key: 'severity', value: 'high' }],
                    agent_model_override: null,
                    agent_reasoning_effort_override: null
                }),
            }).then(async response => {
                done(response.ok ? 'ok' : await response.text());
            }).catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to create structured-lane matching item")?
        .convert::<String>()
        .context("failed to read structured-lane item setup response")?;
    assert_that!(created).is_equal_to("ok".to_owned());
    Ok(())
}

async fn assert_structured_swim_lane_filter_board_behaviour(
    driver: &WebDriver,
    app: &DispatchTestApp,
) -> Result<(), Report> {
    driver
        .goto(app.url("/?project=demo"))
        .await
        .context("failed to open board after structured swim-lane edit")?;
    find(
        driver,
        By::XPath("//section[contains(@class, 'lane')]//h2[.='Filtered']"),
    )
    .await?;
    let summary = driver
        .execute(
            r#"
            const lane = Array.from(document.querySelectorAll('.lane'))
                .find((lane) => lane.querySelector('.lane-header h2')?.textContent?.trim() === 'Filtered');
            return [
                `lane=${Boolean(lane)}`,
                `item=${Boolean(lane?.textContent?.includes('Structured lane item'))}`,
                `add=${Boolean(lane?.querySelector('.lane-add'))}`,
            ].join(';');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect structured swim-lane board state")?
        .convert::<String>()
        .context("failed to read structured swim-lane board summary")?;
    assert_that!(summary).is_equal_to("lane=true;item=true;add=true".to_owned());

    let clicked = driver
        .execute(
            r#"
            const lane = Array.from(document.querySelectorAll('.lane'))
                .find((lane) => lane.querySelector('.lane-header h2')?.textContent?.trim() === 'Filtered');
            lane?.querySelector('.lane-add')?.click();
            return lane ? 'ok' : 'missing lane';
            "#,
            Vec::new(),
        )
        .await
        .context("failed to click structured swim-lane add button")?
        .convert::<String>()
        .context("failed to read structured swim-lane click result")?;
    assert_that!(clicked).is_equal_to("ok".to_owned());
    find(driver, By::Css("#new-item-modal select[name='state']")).await?;
    let state = driver
        .execute(
            r#"
            const select = document.querySelector('#new-item-modal select[name="state"]');
            if (!select) {
                throw new Error('missing new item state select');
            }
            return `${select.value}|${Array.from(select.options).map(option => option.value).join(',')}`;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect structured swim-lane preselected state")?
        .convert::<String>()
        .context("failed to read structured swim-lane preselected state")?;
    assert_that!(state).is_equal_to("open|open".to_owned());
    close_clean_new_item_modal(driver).await?;
    Ok(())
}

async fn create_relationship_target_item(driver: &WebDriver) -> Result<i64, Report> {
    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            fetch('/api/projects/demo/items', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    title: 'Relationship target',
                    description: 'Created as a browser-test relationship target',
                    state: 'open',
                    agent_model_override: null,
                    agent_reasoning_effort_override: null,
                }),
            }).then(async response => {
                const body = await response.json();
                done(`${response.status}|${body.id ?? body.error ?? '<missing>'}`);
            }).catch(error => done(`error|${error}`));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to create relationship target through browser-test API request")?
        .convert::<String>()
        .context("failed to read relationship target setup response")?;
    let Some((status, value)) = result.split_once('|') else {
        bail!("unexpected relationship target API response {result:?}");
    };
    assert_that!(status).is_equal_to("200");
    Ok(value
        .parse::<i64>()
        .context("failed to parse relationship target item id")?)
}

async fn seed_memory_history(driver: &WebDriver) -> Result<(), Report> {
    let seeded = driver
        .execute_async(
            r#"
            const done = arguments[0];
            async function setMemory(body) {
                return await fetch('/projects/demo/memory', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                    body: new URLSearchParams({ body }),
                });
            }
            (async () => {
                const first = await setMemory('Initial shared memory');
                const second = await setMemory('Current shared memory');
                done(first.ok && second.ok ? 'ok' : `failed: ${first.status} ${second.status}`);
            })().catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to seed project memory through browser-test setup request")?
        .convert::<String>()
        .context("failed to read memory seed response")?;
    assert_that!(seeded).is_equal_to("ok".to_owned());
    Ok(())
}

async fn seed_system_prompt_history(driver: &WebDriver) -> Result<(), Report> {
    let seeded = driver
        .execute_async(
            r#"
            const done = arguments[0];
            async function setSystemPrompt(body) {
                return await fetch('/projects/demo/system-prompt', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                    body: new URLSearchParams({ body }),
                });
            }
            (async () => {
                const first = await setSystemPrompt('Initial project prompt');
                const second = await setSystemPrompt('Current project prompt');
                done(first.ok && second.ok ? 'ok' : `failed: ${first.status} ${second.status}`);
            })().catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to seed project system prompt through browser-test setup request")?
        .convert::<String>()
        .context("failed to read system prompt seed response")?;
    assert_that!(seeded).is_equal_to("ok".to_owned());
    Ok(())
}

async fn assert_user_comment_create_flow(driver: &WebDriver) -> Result<(), Report> {
    driver
        .execute(
            r#"
            const form = document.querySelector('section.comments form');
            const author = form?.querySelector('input[name="author_name"]');
            const body = form?.querySelector('textarea[name="body"]');
            if (!form || !author || !body) {
                throw new Error('missing user comment form');
            }
            window.__dispatchCommentUrl = window.location.href;
            author.value = 'Browser user';
            author.dispatchEvent(new Event('input', { bubbles: true }));
            body.value = 'Typed browser comment';
            body.dispatchEvent(new Event('input', { bubbles: true }));
            form.requestSubmit();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to submit typed user comment")?;
    find(
        driver,
        By::XPath(
            "//section[contains(@class, 'comments')]//p[normalize-space()='Typed browser comment']",
        ),
    )
    .await?;
    let summary = driver
        .execute(
            r#"
            const form = document.querySelector('section.comments form');
            return [
                `sameUrl=${window.location.href === window.__dispatchCommentUrl}`,
                `authorReset=${form?.querySelector('input[name="author_name"]')?.value === ''}`,
                `bodyReset=${form?.querySelector('textarea[name="body"]')?.value === ''}`,
            ].join(';');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect typed user comment result")?
        .convert::<String>()
        .context("failed to read typed user comment summary")?;
    assert_that!(summary).is_equal_to("sameUrl=true;authorReset=true;bodyReset=true".to_owned());
    Ok(())
}

async fn add_agent_comment(driver: &WebDriver) -> Result<(), Report> {
    let created = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const itemId = window.location.pathname.match(/\/items\/(\d+)$/)?.[1];
            if (!itemId) {
                done('missing item id');
                return;
            }
            fetch(`/api/projects/demo/items/${itemId}/comments`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    author_type: 'agent',
                    author_name: 'dispatch-run-60',
                    body: 'Agent progress from browser test',
                }),
            }).then(async response => {
                done(response.ok ? 'ok' : await response.text());
            }).catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to add agent comment through API browser-test request")?
        .convert::<String>()
        .context("failed to read agent comment setup response")?;
    assert_that!(created).is_equal_to("ok".to_owned());
    Ok(())
}

async fn claim_current_item(driver: &WebDriver) -> Result<(), Report> {
    let claimed = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const itemId = window.location.pathname.match(/\/items\/(\d+)$/)?.[1];
            if (!itemId) {
                done('missing item id');
                return;
            }
            (async () => {
                const state = 'browser-claimable';
                const moveResponse = await fetch(`/api/projects/demo/items/${itemId}`, {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ state }),
                });
                if (!moveResponse.ok) {
                    done(await moveResponse.text());
                    return;
                }

                const claimResponse = await fetch('/api/projects/demo/items/claim', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        agent_id: 'dispatch-run-60',
                        state,
                    }),
                });
                if (!claimResponse.ok) {
                    done(await claimResponse.text());
                    return;
                }
                const payload = await claimResponse.json();
                if (!payload.item || String(payload.item.id) !== itemId) {
                    done(`claimed wrong item: ${payload.item?.id ?? 'none'}`);
                    return;
                }
                done('ok');
            })().catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to claim current item through API browser-test request")?
        .convert::<String>()
        .context("failed to read item claim setup response")?;
    assert_that!(claimed).is_equal_to("ok".to_owned());
    Ok(())
}

async fn assert_memory_history_selector_behaviour(driver: &WebDriver) -> Result<(), Report> {
    let mut ready = false;
    for _ in 0..20 {
        let status = driver
            .execute_async(
                r#"
                const done = arguments[0];
                const textarea = document.querySelector('textarea.project-memory-text');
                if (!textarea) {
                    done('missing textarea');
                    return;
                }
                textarea.value = 'Unsaved current memory';
                textarea.dispatchEvent(new Event('input', { bubbles: true }));
                setTimeout(() => {
                    done(textarea.classList.contains('dirty') ? 'ready' : 'waiting');
                }, 100);
                "#,
                Vec::new(),
            )
            .await
            .context("failed to probe memory history hydration state")?
            .convert::<String>()
            .context("failed to read memory history hydration status")?;

        if status == "ready" {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    if !ready {
        bail!("memory history selector did not become interactive");
    }

    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const select = document.querySelector('#project-memory-version');
            const textarea = document.querySelector('textarea.project-memory-text');
            const save = document.querySelector("form[action='/projects/demo/memory'] button");
            if (!select || !textarea || !save) {
                done('missing memory controls');
                return;
            }
            if (select.value !== 'current') {
                done(`expected current selection, got ${select.value}`);
                return;
            }
            if (select.options.length < 3) {
                done(`expected current plus history options, got ${select.options.length}`);
                return;
            }
            if (textarea.value !== 'Unsaved current memory') {
                done(`expected cached draft before switch, got ${textarea.value}`);
                return;
            }

            select.value = select.options[2].value;
            select.dispatchEvent(new Event('change', { bubbles: true }));
            setTimeout(() => {
                if (textarea.value !== 'Initial shared memory') {
                    done(`expected historical memory, got ${textarea.value}`);
                    return;
                }
                if (!textarea.readOnly) {
                    done('historical memory textarea was editable');
                    return;
                }
                if (textarea.classList.contains('dirty')) {
                    done('historical memory textarea was highlighted');
                    return;
                }
                if (!save.disabled) {
                    done('save button was enabled for historical memory');
                    return;
                }

                select.value = 'current';
                select.dispatchEvent(new Event('change', { bubbles: true }));
                setTimeout(() => {
                    if (textarea.value !== 'Unsaved current memory') {
                        done(`expected cached current draft, got ${textarea.value}`);
                        return;
                    }
                    if (textarea.readOnly) {
                        done('current memory textarea stayed read-only');
                        return;
                    }
                    if (!textarea.classList.contains('dirty')) {
                        done('current memory draft was not highlighted');
                        return;
                    }
                    if (save.disabled) {
                        done('save button stayed disabled for current memory');
                        return;
                    }
                    done('ok');
                }, 100);
            }, 100);
            "#,
            Vec::new(),
        )
        .await
        .context("failed to verify memory history selector behaviour")?
        .convert::<String>()
        .context("failed to read memory history selector result")?;
    assert_that!(result).is_equal_to("ok".to_owned());
    Ok(())
}

async fn assert_system_prompt_history_selector_behaviour(driver: &WebDriver) -> Result<(), Report> {
    let mut ready = false;
    for _ in 0..20 {
        let status = driver
            .execute_async(
                r#"
                const done = arguments[0];
                const textarea = document.querySelector('textarea.project-system-prompt-text');
                if (!textarea) {
                    done('missing textarea');
                    return;
                }
                textarea.value = 'Unsaved current prompt';
                textarea.dispatchEvent(new Event('input', { bubbles: true }));
                setTimeout(() => {
                    done(textarea.classList.contains('dirty') ? 'ready' : 'waiting');
                }, 100);
                "#,
                Vec::new(),
            )
            .await
            .context("failed to probe system prompt history hydration state")?
            .convert::<String>()
            .context("failed to read system prompt history hydration status")?;

        if status == "ready" {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    if !ready {
        bail!("system prompt history selector did not become interactive");
    }

    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const select = document.querySelector('#project-system-prompt-version');
            const textarea = document.querySelector('textarea.project-system-prompt-text');
            const save = document.querySelector("form[action='/projects/demo/system-prompt'] button");
            if (!select || !textarea || !save) {
                done('missing system prompt controls');
                return;
            }
            if (select.value !== 'current') {
                done(`expected current selection, got ${select.value}`);
                return;
            }
            if (select.options.length < 3) {
                done(`expected current plus history options, got ${select.options.length}`);
                return;
            }
            if (textarea.value !== 'Unsaved current prompt') {
                done(`expected cached draft before switch, got ${textarea.value}`);
                return;
            }

            select.value = select.options[2].value;
            select.dispatchEvent(new Event('change', { bubbles: true }));
            setTimeout(() => {
                if (textarea.value !== 'Initial project prompt') {
                    done(`expected historical prompt, got ${textarea.value}`);
                    return;
                }
                if (!textarea.readOnly) {
                    done('historical system prompt textarea was editable');
                    return;
                }
                if (textarea.classList.contains('dirty')) {
                    done('historical system prompt textarea was highlighted');
                    return;
                }
                if (!save.disabled) {
                    done('save button was enabled for historical system prompt');
                    return;
                }

                select.value = 'current';
                select.dispatchEvent(new Event('change', { bubbles: true }));
                setTimeout(() => {
                    if (textarea.value !== 'Unsaved current prompt') {
                        done(`expected cached current draft, got ${textarea.value}`);
                        return;
                    }
                    if (textarea.readOnly) {
                        done('current system prompt textarea stayed read-only');
                        return;
                    }
                    if (!textarea.classList.contains('dirty')) {
                        done('current system prompt draft was not highlighted');
                        return;
                    }
                    if (save.disabled) {
                        done('save button stayed disabled for current system prompt');
                        return;
                    }
                    done('ok');
                }, 100);
            }, 100);
            "#,
            Vec::new(),
        )
        .await
        .context("failed to verify system prompt history selector behaviour")?
        .convert::<String>()
        .context("failed to read system prompt history selector result")?;
    assert_that!(result).is_equal_to("ok".to_owned());
    Ok(())
}

async fn assert_frontend_route_navigation_renders_page(driver: &WebDriver) -> Result<(), Report> {
    click(driver, By::Css(".top-nav a[href='/projects?project=demo']")).await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
    )
    .await?;
    click(driver, By::Css(".top-nav a[href='/?project=demo']")).await?;
    find(driver, By::Css("section.board")).await?;

    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const reactiveWarnings = [];
            const originalWarn = console.warn;
            const restoreWarn = () => console.warn = originalWarn;
            console.warn = (...args) => {
                const message = args.map(String).join(' ');
                if (message.includes('outside a reactive tracking context')) {
                    reactiveWarnings.push(message);
                }
                originalWarn.apply(console, args);
            };
            const link = document.querySelector(".top-nav a[href='/projects?project=demo']");
            if (!link) {
                restoreWarn();
                done('missing projects link');
                return;
            }
            link.click();

            const deadline = Date.now() + 5000;
            const check = () => {
                if (document.querySelector("[data-crudkit-leptos='projects'] .crud-nav")) {
                    restoreWarn();
                    done(reactiveWarnings.length === 0
                        ? 'rendered'
                        : `reactive-warning:${reactiveWarnings.join('\n')}`);
                    return;
                }
                if (Date.now() > deadline) {
                    restoreWarn();
                    done(`timeout;url=${window.location.href}`);
                    return;
                }
                setTimeout(check, 0);
            };
            check();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to verify frontend route navigation")?
        .convert::<String>()
        .context("failed to read frontend route navigation result")?;
    assert_that!(result).is_equal_to("rendered".to_owned());

    click(driver, By::Css(".top-nav a[href='/?project=demo']")).await?;
    find(driver, By::Css("section.board")).await?;
    Ok(())
}

async fn assert_service_get_results_use_local_storage(driver: &WebDriver) -> Result<(), Report> {
    let result = driver
        .execute(
            r#"
            const required = [
                'dispatch.query.board.v1',
                'dispatch.query.board-items.v1',
                'dispatch.query.projects.v1',
            ];
            return required
                .filter((key) => !localStorage.getItem(key))
                .join(',');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect service GET local-storage caches")?
        .convert::<String>()
        .context("failed to read service GET local-storage cache result")?;
    assert_that!(result).is_empty();
    Ok(())
}

async fn assert_crudkit_create_form_survives_live_event(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::Css("[data-crudkit-leptos='work-items'] .crud-nav button"),
    )
    .await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='work-items'] .crud-input-field"),
    )
    .await?;

    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const editableField = () => {
                const panel = document.querySelector("[data-crudkit-leptos='work-items']");
                const field = panel?.querySelector(".crud-input-field");
                if (!field) {
                    return null;
                }
                if (field.matches('input, textarea')) {
                    return field;
                }
                return field.querySelector('input, textarea');
            };
            const draftInput = editableField();
            if (!draftInput) {
                done('missing work-item create input');
                return;
            }
            draftInput.value = 'Draft survives live event';
            draftInput.dispatchEvent(new Event('keyup', { bubbles: true }));
            draftInput.dispatchEvent(new Event('change', { bubbles: true }));

            fetch('/api/projects/demo/items', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    title: 'Live refresh item',
                    description: 'Created to emit a websocket event',
                    state: 'open',
                    agent_model_override: null,
                    agent_reasoning_effort_override: null
                }),
            }).then(async response => {
                if (!response.ok) {
                    done(await response.text());
                    return;
                }
                const deadline = Date.now() + 5000;
                const check = () => {
                    const currentInput = editableField();
                    const boardUpdated = document.body.textContent.includes('Live refresh item');
                    if (currentInput?.value === 'Draft survives live event' && boardUpdated) {
                        done('ok');
                        return;
                    }
                    if (Date.now() > deadline) {
                        done(`draft=${currentInput?.value ?? '<missing>'}; boardUpdated=${boardUpdated}`);
                        return;
                    }
                    setTimeout(check, 100);
                };
                check();
            }).catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to verify CrudKit create form survives live event")?
        .convert::<String>()
        .context("failed to read CrudKit live event result")?;
    assert_that!(result).is_equal_to("ok".to_owned());
    Ok(())
}

async fn assert_request_error_toast_preserves_page_and_draft(
    driver: &WebDriver,
) -> Result<(), Report> {
    let prepared = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const expectedDraft = 'Draft survives request failure';
            const editableField = () => {
                const panel = document.querySelector("[data-crudkit-leptos='work-items']");
                const field = panel?.querySelector(".crud-input-field");
                if (!field) {
                    return null;
                }
                if (field.matches('input, textarea')) {
                    return field;
                }
                return field.querySelector('input, textarea');
            };
            const draftInput = editableField();
            if (!draftInput) {
                done('missing work-item create input');
                return;
            }
            draftInput.value = expectedDraft;
            draftInput.dispatchEvent(new Event('input', { bubbles: true }));
            draftInput.dispatchEvent(new Event('keyup', { bubbles: true }));
            draftInput.dispatchEvent(new Event('change', { bubbles: true }));

            const originalFetch = window.__dispatchOriginalFetch ?? window.fetch.bind(window);
            window.__dispatchOriginalFetch = originalFetch;
            window.__dispatchFailedFetches = [];
            window.__dispatchFailBoardPageRequest = true;
            window.fetch = (input, init) => {
                const rawUrl = typeof input === 'string' ? input : input?.url ?? String(input);
                const url = new URL(rawUrl, window.location.href);
                if (
                    window.__dispatchFailBoardPageRequest &&
                    url.pathname.startsWith('/leptos/load_board_page')
                ) {
                    window.__dispatchFailBoardPageRequest = false;
                    window.__dispatchFailedFetches.push(url.href);
                    return new Promise((resolve) => {
                        window.__dispatchReleaseBoardPageRequest = () => {
                            resolve(new Response('browser-test injected request failure', {
                                status: 503,
                                statusText: 'Browser Test Failure',
                                headers: { 'content-type': 'text/plain' },
                            }));
                        };
                    });
                }
                return originalFetch(input, init);
            };
            done('ok');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to prepare page-request failure browser-test state")?
        .convert::<String>()
        .context("failed to read page-request failure preparation result")?;
    assert_that!(prepared).is_equal_to("ok".to_owned());

    click(
        driver,
        By::Css(".project-switcher leptonic-select-selected"),
    )
    .await?;
    click(
        driver,
        By::XPath(
            "//div[contains(@class, 'project-switcher')]//leptonic-select-option[contains(., 'Demo Alt')]",
        ),
    )
    .await?;

    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const expectedDraft = 'Draft survives request failure';
            const deadline = Date.now() + 5000;
            const editableField = () => {
                const panel = document.querySelector("[data-crudkit-leptos='work-items']");
                const field = panel?.querySelector(".crud-input-field");
                if (!field) {
                    return null;
                }
                if (field.matches('input, textarea')) {
                    return field;
                }
                return field.querySelector('input, textarea');
            };
            const restoreFetch = () => {
                if (window.__dispatchOriginalFetch) {
                    window.fetch = window.__dispatchOriginalFetch;
                }
                window.__dispatchFailBoardPageRequest = false;
                window.__dispatchReleaseBoardPageRequest = undefined;
            };
            let released = false;
            const check = () => {
                const failedFetches = window.__dispatchFailedFetches ?? [];
                const draftInput = editableField();
                const toast = document.querySelector('leptonic-toast[data-variant="error"]');
                const toastText = toast?.textContent ?? '';
                const topbar = document.querySelector('header.app-topbar');
                const brand = topbar?.querySelector('.brand');
                const navLinks = topbar?.querySelectorAll('.top-nav a').length ?? 0;
                const projectSwitcher = topbar?.querySelector('.project-switcher');
                const codexStatus = topbar?.querySelector('.topbar-codex');
                const automationStatus = topbar?.querySelector('.topbar-automation');
                const pageShell = document.querySelector('main.page-shell');
                const board = document.querySelector('section.board');
                const pageLoading = Array.from(document.querySelectorAll('main.page-shell'))
                    .some((page) => page.textContent.trim() === 'Loading...');
                if (
                    !released &&
                    failedFetches.length === 1 &&
                    topbar &&
                    brand &&
                    navLinks === 5 &&
                    projectSwitcher &&
                    codexStatus &&
                    automationStatus &&
                    pageShell &&
                    board &&
                    !toast &&
                    draftInput?.value === expectedDraft &&
                    !pageLoading &&
                    window.__dispatchReleaseBoardPageRequest
                ) {
                    released = true;
                    window.__dispatchReleaseBoardPageRequest();
                }
                if (
                    released &&
                    toastText.includes('Request failed') &&
                    topbar &&
                    brand &&
                    navLinks === 5 &&
                    projectSwitcher &&
                    codexStatus &&
                    automationStatus &&
                    pageShell &&
                    board &&
                    draftInput?.value === expectedDraft &&
                    !pageLoading
                ) {
                    restoreFetch();
                    done('ok');
                    return;
                }
                if (Date.now() > deadline) {
                    const report = [
                        `failedFetches=${failedFetches.join(',')}`,
                        `released=${released}`,
                        `toast=${toastText}`,
                        `topbar=${Boolean(topbar)}`,
                        `brand=${Boolean(brand)}`,
                        `navLinks=${navLinks}`,
                        `projectSwitcher=${Boolean(projectSwitcher)}`,
                        `codexStatus=${Boolean(codexStatus)}`,
                        `automationStatus=${Boolean(automationStatus)}`,
                        `pageShell=${Boolean(pageShell)}`,
                        `board=${Boolean(board)}`,
                        `draft=${draftInput?.value ?? '<missing>'}`,
                        `pageLoading=${pageLoading}`,
                        `url=${window.location.href}`,
                    ].join('; ');
                    window.__dispatchReleaseBoardPageRequest?.();
                    restoreFetch();
                    done(report);
                    return;
                }
                setTimeout(check, 100);
            };
            check();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to verify failed page requests are rendered")?
        .convert::<String>()
        .context("failed to read failed page-request result")?;
    assert_that!(result).is_equal_to("ok".to_owned());
    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await
        .context("failed to dismiss project switcher after request-failure assertion")?;
    wait_for_no_modal_backdrop_blocking(driver, "after request-failure assertion").await?;

    Ok(())
}

async fn assert_auto_commit_toggle_updates_without_navigation(
    driver: &WebDriver,
) -> Result<(), Report> {
    let initial = driver
        .execute(
            r#"
            const button = document.querySelector('.topbar-auto-commit[role="switch"]');
            const checkbox = document.querySelector('#project-auto-commit');
            window.__dispatchAutoCommitMarker = 'alive';
            window.__dispatchAutoCommitUrl = window.location.href;
            return `${button?.getAttribute('aria-checked') ?? 'missing'}|${checkbox?.checked ?? 'missing'}`;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect initial Auto-Commit state")?
        .convert::<String>()
        .context("failed to read initial Auto-Commit state")?;
    assert_that!(initial).is_equal_to("true|true".to_owned());

    click(driver, By::Css(".topbar-auto-commit[role='switch']")).await?;

    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const deadline = Date.now() + 5000;
            async function check() {
                const button = document.querySelector('.topbar-auto-commit[role="switch"]');
                const checkbox = document.querySelector('#project-auto-commit');
                const marker = window.__dispatchAutoCommitMarker;
                const sameUrl = window.location.href === window.__dispatchAutoCommitUrl;
                const checked = button?.getAttribute('aria-checked') ?? 'missing';
                const settingsChecked = checkbox?.checked ?? 'missing';
                let persisted = 'not checked';
                try {
                    const response = await fetch('/api/projects/demo/settings');
                    persisted = response.ok ? (await response.json()).auto_commit : `status ${response.status}`;
                } catch (error) {
                    persisted = String(error);
                }

                if (
                    marker === 'alive' &&
                    sameUrl &&
                    checked === 'false' &&
                    settingsChecked === false &&
                    persisted === false
                ) {
                    done('ok');
                    return;
                }
                if (Date.now() > deadline) {
                    done(`marker=${marker}; sameUrl=${sameUrl}; checked=${checked}; settingsChecked=${settingsChecked}; persisted=${persisted}`);
                    return;
                }
                setTimeout(check, 100);
            }
            check();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to verify Auto-Commit toggle behaviour")?
        .convert::<String>()
        .context("failed to read Auto-Commit toggle result")?;
    assert_that!(result).is_equal_to("ok".to_owned());

    Ok(())
}

async fn assert_settings_response_omits_refinement_policy(
    driver: &WebDriver,
) -> Result<(), Report> {
    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            fetch('/api/projects/demo/settings')
                .then(async (response) => {
                    if (!response.ok) {
                        done(`status ${response.status}`);
                        return;
                    }
                    const settings = await response.json();
                    const legacyKey = ['allow', 'refinement', 'agents', 'during', 'editing'].join('_');
                    done(Object.hasOwn(settings, legacyKey) ? 'present' : 'absent');
                })
                .catch((error) => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect project settings API response")?
        .convert::<String>()
        .context("failed to read project settings API field check")?;
    assert_that!(result).is_equal_to("absent".to_owned());
    Ok(())
}

async fn create_trigger(driver: &WebDriver) -> Result<(), Report> {
    let created = driver
        .execute_async(
            r#"
            const done = arguments[0];
            fetch('/api/automation_triggers/crud/create-one', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    entity: {
                        project_id: 1,
                        name: 'refine-new',
                        enabled: true,
                        activation: 'work_item_created',
                        effect: 'consume_work',
                        schedule: '@every 15s',
                        tool_name: 'codex',
                        mutability: 'read_only',
                        prompt: 'Refine new work items.',
                        work_item_selector: '{"All":[{"column_name":"state","operator":"=","value":{"String":"open"}}]}',
                        priority: 0
                    }
                }),
            }).then(async response => {
                done(response.ok ? 'ok' : await response.text());
            }).catch(error => done(String(error)));
            "#,
            Vec::new(),
        )
        .await
        .context("failed to create trigger through CrudKit browser-test setup request")?
        .convert::<String>()
        .context("failed to read trigger setup response")?;
    assert_that!(created).is_equal_to("ok".to_owned());
    Ok(())
}

async fn open_new_item_modal(driver: &WebDriver) -> Result<(), Report> {
    let mut last_state = inspect_new_item_modal_state(driver).await?;
    for _ in 0..20 {
        if last_state.starts_with("modalVisible=true;") && last_state.contains("formReady=true") {
            return Ok(());
        }
        if !last_state.starts_with("modalVisible=true;") {
            click_css_after_modal_backdrops_clear(
                driver,
                ".lane:nth-child(1) .lane-add",
                "opening new item modal",
            )
            .await?;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        last_state = inspect_new_item_modal_state(driver).await?;
    }
    bail!("new item modal did not open: {last_state}");
}

async fn assert_lane_add_button_count(driver: &WebDriver, expected: usize) -> Result<(), Report> {
    let count = driver
        .execute(
            "return String(document.querySelectorAll('.lane .lane-add').length);",
            Vec::new(),
        )
        .await
        .context("failed to count lane add buttons")?
        .convert::<String>()
        .context("failed to read lane add button count")?;
    assert_that!(count).is_equal_to(expected.to_string());
    Ok(())
}

async fn assert_board_shell_uses_viewport_width(driver: &WebDriver) -> Result<(), Report> {
    driver
        .set_window_rect(0, 0, 1800, 1000)
        .await
        .context("failed to widen browser test window")?;
    let summary = driver
        .execute(
            r#"
            const shell = document.querySelector('main.page-shell');
            if (!shell) {
                throw new Error('missing page shell');
            }
            const shellWidth = Math.round(shell.getBoundingClientRect().width);
            const viewportWidth = document.documentElement.clientWidth;
            return `${shellWidth}|${viewportWidth}`;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect board shell width")?
        .convert::<String>()
        .context("failed to read board shell width")?;
    let Some((shell_width, viewport_width)) = summary.split_once('|') else {
        bail!("failed to parse board shell width summary {summary:?}");
    };
    let shell_width = shell_width
        .parse::<i64>()
        .context("failed to parse shell width")?;
    let viewport_width = viewport_width
        .parse::<i64>()
        .context("failed to parse viewport width")?;
    if shell_width < viewport_width - 1 {
        bail!("board shell width {shell_width}px did not fill viewport width {viewport_width}px");
    }
    Ok(())
}

async fn assert_board_layout_is_lane_first(driver: &WebDriver) -> Result<(), Report> {
    let desktop = driver
        .execute(
            r#"
            const main = document.querySelector('main.page-shell');
            const workspace = main?.querySelector('.workspace-bar');
            const board = main?.querySelector('section.board');
            const laneAdd = board?.querySelector('.lane-add');
            if (!main || !workspace || !board || !laneAdd) {
                throw new Error('missing compact Board layout');
            }
            const workspaceBeforeBoard = Boolean(
                workspace.compareDocumentPosition(board) & Node.DOCUMENT_POSITION_FOLLOWING
            );
            return [
                `heading=${Boolean(main.querySelector('h1'))}`,
                `toolbar=${Boolean(main.querySelector('.board-toolbar'))}`,
                `runtime=${Boolean(main.querySelector('.runtime-panel'))}`,
                `workspaceBeforeBoard=${workspaceBeforeBoard}`,
                `workspaceHeight=${Math.round(workspace.getBoundingClientRect().height)}`,
                `addInHeader=${Boolean(laneAdd.closest('.lane-header'))}`,
            ].join(';');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect compact desktop Board layout")?
        .convert::<String>()
        .context("failed to read compact desktop Board layout")?;
    let expected_prefix =
        "heading=false;toolbar=false;runtime=false;workspaceBeforeBoard=true;workspaceHeight=";
    if !desktop.starts_with(expected_prefix) || !desktop.ends_with(";addInHeader=true") {
        bail!("unexpected compact desktop Board layout: {desktop}");
    }
    let Some(workspace_height) = desktop
        .strip_prefix(expected_prefix)
        .and_then(|rest| rest.strip_suffix(";addInHeader=true"))
        .and_then(|height| height.parse::<i64>().ok())
    else {
        bail!("failed to parse compact workspace bar height from {desktop}");
    };
    if workspace_height >= 100 {
        bail!("workspace bar was not compact: {workspace_height}px high");
    }

    driver
        .set_window_rect(0, 0, 390, 900)
        .await
        .context("failed to resize browser for narrow Board layout")?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let mobile_add = driver
        .execute(
            r#"
            const laneAdd = document.querySelector('.lane .lane-add');
            if (!laneAdd) {
                throw new Error('missing narrow Board lane add control');
            }
            const style = getComputedStyle(laneAdd);
            return `${style.opacity}|${style.pointerEvents}|${Boolean(laneAdd.closest('.lane-header'))}`;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect narrow Board lane add control")?
        .convert::<String>()
        .context("failed to read narrow Board lane add control")?;
    assert_that!(mobile_add).is_equal_to("1|auto|true".to_owned());

    driver
        .set_window_rect(0, 0, 1800, 1000)
        .await
        .context("failed to restore desktop browser size")?;
    Ok(())
}

async fn assert_lane_new_item_state(driver: &WebDriver) -> Result<(), Report> {
    let summary = driver
        .execute(
            r#"
            const select = document.querySelector('#new-item-modal select[name="state"]');
            if (!select) {
                throw new Error('missing new item state select');
            }
            return `${select.value}|${Array.from(select.options).map(option => option.value).join(',')}`;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect lane-scoped new item state")?
        .convert::<String>()
        .context("failed to read lane-scoped new item state")?;
    assert_that!(summary).is_equal_to("idea|idea".to_owned());
    Ok(())
}

async fn assert_new_item_modal_actions(driver: &WebDriver) -> Result<(), Report> {
    let summary = driver
        .execute(
            r#"
            const modal = document.querySelector('#new-item-modal');
            if (!modal) {
                throw new Error('missing new item modal');
            }
            const headerButton = modal.querySelector('leptonic-modal-header button');
            const bodySaveButton = modal.querySelector('leptonic-modal-body .crud-nav button');
            const footerButtons = Array.from(modal.querySelectorAll('leptonic-modal-footer button'));
            const footerButtonTexts = footerButtons
                .map(button => (button.textContent ?? '').replace(/\s+/g, ' ').trim())
                .join('|');
            return [
                `headerIcon=${Boolean(headerButton?.querySelector('svg'))}`,
                `headerText=${(headerButton?.textContent ?? '').replace(/\s+/g, ' ').trim() || '<empty>'}`,
                `bodySave=${Boolean(bodySaveButton)}`,
                `footerButtons=${footerButtonTexts}`,
            ].join(';');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect new item modal actions")?
        .convert::<String>()
        .context("failed to read new item modal action summary")?;
    assert_that!(summary).is_equal_to(
        "headerIcon=true;headerText=<empty>;bodySave=false;footerButtons=Cancel|Speichern"
            .to_owned(),
    );
    Ok(())
}

async fn assert_lane_add_preselects_state(driver: &WebDriver) -> Result<(), Report> {
    click_css_after_modal_backdrops_clear(
        driver,
        ".lane:nth-child(2) .lane-add",
        "opening lane-preselected new item modal",
    )
    .await?;
    find(driver, By::Css("#new-item-modal select[name='state']")).await?;
    let summary = driver
        .execute(
            r#"
            const select = document.querySelector('#new-item-modal select[name="state"]');
            if (!select) {
                throw new Error('missing new item state select');
            }
            return `${select.value}|${Array.from(select.options).map(option => option.value).join(',')}`;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect lane-preselected new item state")?
        .convert::<String>()
        .context("failed to read lane-preselected new item state")?;
    assert_that!(summary).is_equal_to("open|open".to_owned());
    close_clean_new_item_modal(driver).await?;
    Ok(())
}

async fn assert_new_item_modal_dirty_leave_protection(driver: &WebDriver) -> Result<(), Report> {
    open_new_item_modal(driver).await?;
    set_input_value(
        driver,
        "#new-item-modal .crud-input-field",
        "Unsaved modal title",
    )
    .await?;

    click(
        driver,
        By::Css("#new-item-modal leptonic-modal-header button"),
    )
    .await?;
    find_leave_modal(driver).await?;
    click_leave_modal_cancel(driver).await?;
    assert_new_item_title_value(driver, "Unsaved modal title").await?;

    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await
        .context("failed to press Escape for new item modal")?;
    find_leave_modal(driver).await?;
    click_leave_modal_cancel(driver).await?;
    assert_new_item_title_value(driver, "Unsaved modal title").await?;

    click_backdrop(driver).await?;
    find_leave_modal(driver).await?;
    click_leave_modal_accept(driver).await?;
    wait_for_new_item_modal_closed(driver).await?;

    open_new_item_modal(driver).await?;
    append_new_item_initial_label(driver, "area", "unsaved").await?;
    click(
        driver,
        By::Css("#new-item-modal leptonic-modal-header button"),
    )
    .await?;
    find_leave_modal(driver).await?;
    click_leave_modal_cancel(driver).await?;
    assert_new_item_initial_label_value(driver, "area", "unsaved").await?;

    click_backdrop(driver).await?;
    find_leave_modal(driver).await?;
    click_leave_modal_accept(driver).await?;
    wait_for_new_item_modal_closed(driver).await?;
    Ok(())
}

async fn assert_item_detail_dirty_leave_protection(driver: &WebDriver) -> Result<(), Report> {
    set_input_value(
        driver,
        "section.item-settings .crud-input-field",
        "Unsaved detail title",
    )
    .await?;

    click(driver, By::Css("button.item-board-link")).await?;
    find_leave_modal(driver).await?;
    click_leave_modal_cancel(driver).await?;
    assert_source_contains(driver, "Unsaved detail title").await?;

    click(driver, By::Css("button.item-board-link")).await?;
    find_leave_modal(driver).await?;
    click_leave_modal_accept(driver).await?;
    find(driver, By::Css("section.board")).await?;
    Ok(())
}

async fn close_clean_new_item_modal(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::Css("#new-item-modal leptonic-modal-footer button"),
    )
    .await?;
    wait_for_new_item_modal_closed(driver).await
}

async fn click_new_item_save(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::XPath("//leptonic-modal[@id='new-item-modal']//button[contains(., 'Speichern')]"),
    )
    .await
}

async fn find_leave_modal(driver: &WebDriver) -> Result<(), Report> {
    find(
        driver,
        By::XPath("//leptonic-modal[contains(., 'Ungespeicherte Änderungen')]"),
    )
    .await
    .map(|_| ())
}

async fn click_leave_modal_cancel(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::XPath(
            "//leptonic-modal[contains(., 'Ungespeicherte Änderungen')]//button[contains(., 'Zurück')]",
        ),
    )
    .await
}

async fn click_leave_modal_accept(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::XPath(
            "//leptonic-modal[contains(., 'Ungespeicherte Änderungen')]//button[contains(., 'Verlassen')]",
        ),
    )
    .await
}

async fn assert_new_item_title_value(driver: &WebDriver, expected: &str) -> Result<(), Report> {
    let value = driver
        .execute(
            r#"
            const editable = (field) => {
                if (!field) {
                    return null;
                }
                if (field.matches('input, textarea, select')) {
                    return field;
                }
                return field.querySelector('input, textarea, select');
            };
            const field = document.querySelector('#new-item-modal .crud-input-field');
            const input = editable(field);
            return input?.value ?? '<missing>';
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect new item title draft")?
        .convert::<String>()
        .context("failed to read new item title draft")?;
    assert_that!(value).is_equal_to(expected.to_owned());
    Ok(())
}

async fn append_new_item_initial_label(
    driver: &WebDriver,
    key: &str,
    value: &str,
) -> Result<(), Report> {
    let script = format!(
        r#"
        const done = arguments[0];
        const key = {key:?};
        const value = {value:?};
        const add = document.querySelector('#new-item-modal .initial-label-add');
        if (!add) {{
            done('missing add button');
            return;
        }}
        const before = document.querySelectorAll('#new-item-modal .initial-label-row').length;
        add.click();
        const deadline = Date.now() + 5000;
        const setValue = (input, next) => {{
            input.value = next;
            input.setAttribute('value', next);
            input.dispatchEvent(new Event('input', {{ bubbles: true }}));
            input.dispatchEvent(new Event('change', {{ bubbles: true }}));
        }};
        const fill = () => {{
            const rows = document.querySelectorAll('#new-item-modal .initial-label-row');
            if (rows.length <= before) {{
                if (Date.now() > deadline) {{
                    done(`row count stayed at ${{before}}`);
                    return;
                }}
                setTimeout(fill, 100);
                return;
            }}
            const row = rows[rows.length - 1];
            const keyInput = row.querySelector('.initial-label-key');
            const valueInput = row.querySelector('.initial-label-value');
            if (!keyInput || !valueInput) {{
                done('missing row inputs');
                return;
            }}
            setValue(keyInput, key);
            setValue(valueInput, value);
            done('ok');
        }};
        fill();
        "#
    );
    let result = driver
        .execute_async(script, Vec::new())
        .await
        .context("failed to append new item initial label")?
        .convert::<String>()
        .context("failed to read initial label append result")?;
    if result != "ok" {
        bail!("failed to append new item initial label: {result}");
    }
    Ok(())
}

async fn assert_new_item_initial_label_value(
    driver: &WebDriver,
    expected_key: &str,
    expected_value: &str,
) -> Result<(), Report> {
    let summary = driver
        .execute(
            r#"
            const row = document.querySelector('#new-item-modal .initial-label-row');
            const key = row?.querySelector('.initial-label-key')?.value ?? '<missing>';
            const value = row?.querySelector('.initial-label-value')?.value ?? '<missing>';
            return `${key}|${value}`;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect initial label draft")?
        .convert::<String>()
        .context("failed to read initial label draft")?;
    assert_that!(summary).is_equal_to(format!("{expected_key}|{expected_value}"));
    Ok(())
}

async fn assert_board_card_contains(
    driver: &WebDriver,
    title: &str,
    expected: &str,
) -> Result<(), Report> {
    let script = format!(
        r#"
        const title = {title:?};
        const expected = {expected:?};
        const link = Array.from(document.querySelectorAll('article.card a'))
            .find((link) => (link.textContent ?? '').includes(title));
        const card = link?.closest('article.card');
        return card ? String((card.textContent ?? '').includes(expected)) : 'missing-card';
        "#
    );
    let result = driver
        .execute(script, Vec::new())
        .await
        .context("failed to inspect board card labels")?
        .convert::<String>()
        .context("failed to read board card label summary")?;
    assert_that!(result).is_equal_to("true".to_owned());
    Ok(())
}

async fn click_backdrop(driver: &WebDriver) -> Result<(), Report> {
    driver
        .action_chain()
        .move_to(4, 4)
        .click()
        .perform()
        .await
        .context("failed to click modal backdrop")?;
    Ok(())
}

async fn wait_for_new_item_modal_closed(driver: &WebDriver) -> Result<(), Report> {
    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const deadline = Date.now() + 5000;
            const isVisible = (element) => {
                if (!element) {
                    return false;
                }
                const style = getComputedStyle(element);
                const rect = element.getBoundingClientRect();
                return style.display !== 'none' &&
                    style.visibility !== 'hidden' &&
                    rect.width > 0 &&
                    rect.height > 0;
            };
            const check = () => {
                const modal = document.querySelector('leptonic-modal#new-item-modal');
                const modalVisible = isVisible(modal);
                const backdropState = Array.from(
                    document.querySelectorAll('leptonic-modal-backdrop')
                ).map((backdrop) => {
                    const style = getComputedStyle(backdrop);
                    return {
                        visible: isVisible(backdrop),
                        blocking: style.pointerEvents !== 'none',
                    };
                });
                const backdropVisible = backdropState.some((state) => state.visible);
                const backdropBlocking = backdropState.some((state) => state.blocking);
                const hit = document.elementFromPoint(
                    Math.max(1, document.documentElement.clientWidth - 68),
                    143
                );
                const hitBackdrop = hit?.tagName === 'LEPTONIC-MODAL-BACKDROP' ||
                    Boolean(hit?.closest?.('leptonic-modal-backdrop'));
                if (!modalVisible && !backdropVisible && !backdropBlocking && !hitBackdrop) {
                    done('closed');
                    return;
                }
                if (Date.now() > deadline) {
                    const host = document.querySelector('leptonic-modal-host');
                    const hostStyle = host ? getComputedStyle(host) : null;
                    const modalSummary = Array.from(document.querySelectorAll('leptonic-modal'))
                        .map((modal) => {
                            const style = getComputedStyle(modal);
                            const text = (modal.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 80);
                            return `${modal.id || '<no-id>'}:${style.display}:${style.visibility}:${text}`;
                        })
                        .join(' || ');
                    done([
                        `still-open: modal=${modalVisible}`,
                        `backdrop=${backdropVisible}`,
                        `blocking=${backdropBlocking}`,
                        `hit=${hit?.tagName ?? '<none>'}`,
                        `hostHasModals=${host?.getAttribute('data-has-modals') ?? '<missing>'}`,
                        `hostDisplay=${hostStyle?.display ?? '<missing>'}`,
                        `modals=${modalSummary}`,
                    ].join('; '));
                    return;
                }
                setTimeout(check, 100);
            };
            check();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to wait for new item modal close")?
        .convert::<String>()
        .context("failed to read new item modal close state")?;
    assert_that!(result).is_equal_to("closed".to_owned());
    Ok(())
}

async fn wait_for_no_modal_backdrop_blocking(
    driver: &WebDriver,
    context: &str,
) -> Result<(), Report> {
    let result = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const deadline = Date.now() + 3000;
            const check = () => {
                const hit = document.elementFromPoint(
                    Math.max(1, document.documentElement.clientWidth - 68),
                    143
                );
                const blocking = hit?.tagName === 'LEPTONIC-MODAL-BACKDROP' ||
                    Boolean(hit?.closest?.('leptonic-modal-backdrop'));
                if (!blocking) {
                    done('clear');
                    return;
                }
                if (Date.now() > deadline) {
                    const modalSummary = Array.from(document.querySelectorAll('leptonic-modal'))
                        .map((modal) => {
                            const style = getComputedStyle(modal);
                            const rect = modal.getBoundingClientRect();
                            const text = (modal.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 80);
                            return `${modal.id || '<no-id>'}:${style.display}:${style.visibility}:${Math.round(rect.width)}x${Math.round(rect.height)}:${text}`;
                        })
                        .join(' || ');
                    const selectSummary = Array.from(document.querySelectorAll('leptonic-select, leptonic-select-overlay, leptonic-select-options'))
                        .map((select) => {
                            const style = getComputedStyle(select);
                            const rect = select.getBoundingClientRect();
                            const text = (select.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 80);
                            return `${select.tagName}:${style.display}:${style.visibility}:${Math.round(rect.width)}x${Math.round(rect.height)}:${text}`;
                        })
                        .join(' || ');
                    done(`blocked-by=${hit?.tagName ?? '<none>'}; modals=${modalSummary}; selects=${selectSummary}`);
                    return;
                }
                setTimeout(check, 100);
            };
            check();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to wait for modal backdrop hit-test")?
        .convert::<String>()
        .context("failed to read modal backdrop hit-test state")?;
    if result != "clear" {
        bail!("modal backdrop still blocking {context}: {result}");
    }
    Ok(())
}

async fn click_css_after_modal_backdrops_clear(
    driver: &WebDriver,
    selector: &str,
    context: &str,
) -> Result<(), Report> {
    let mut last_error = None;
    for _ in 0..20 {
        wait_for_no_modal_backdrop_blocking(driver, context).await?;
        let element = find(driver, By::Css(selector)).await?;
        element
            .scroll_into_view()
            .await
            .context("failed to scroll browser-test element into view")?;
        driver
            .action_chain()
            .move_to_element_center(&element)
            .perform()
            .await
            .context("failed to move pointer to browser-test element")?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        match element.click().await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error.to_string());
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }

    bail!(
        "failed to click browser-test element {selector:?} while {context}: {}",
        last_error.unwrap_or_else(|| "no click attempt was made".to_owned())
    );
}

async fn inspect_new_item_modal_state(driver: &WebDriver) -> Result<String, Report> {
    Ok(driver
        .execute(
            r#"
            const modal = document.querySelector('leptonic-modal#new-item-modal');
            const button = document.querySelector('.lane:nth-child(1) .lane-add');
            const host = document.querySelector('leptonic-modal-host');
            const hostStyle = host ? getComputedStyle(host) : null;
            const bodyText = document.body.textContent ?? '';
            const isVisible = (element) => {
                if (!element) {
                    return false;
                }
                const style = getComputedStyle(element);
                const rect = element.getBoundingClientRect();
                return style.display !== 'none' &&
                    style.visibility !== 'hidden' &&
                    rect.width > 0 &&
                    rect.height > 0;
            };
            const editable = (field) => {
                if (!field) {
                    return null;
                }
                if (field.matches('input, textarea, select')) {
                    return field;
                }
                return field.querySelector('input, textarea, select');
            };
            const titleField = modal?.querySelector('.crud-input-field');
            const titleInput = editable(titleField);
            const stateSelect = modal?.querySelector('select[name="state"]');
            return [
                `modalVisible=${isVisible(modal)}`,
                `modal=${Boolean(modal)}`,
                `formReady=${Boolean(titleInput && stateSelect)}`,
                `buttonDisabled=${button?.disabled ?? '<missing>'}`,
                `buttonText=${(button?.textContent ?? '<missing>').trim()}`,
                `hostHasModals=${host?.getAttribute('data-has-modals') ?? '<missing>'}`,
                `hostDisplay=${hostStyle?.display ?? '<missing>'}`,
                `hostContainsModal=${Boolean(host?.querySelector('leptonic-modal'))}`,
                `htmlOverflow=${document.documentElement.style.overflow || '<empty>'}`,
                `lanes=${document.querySelectorAll('.lane').length}`,
                `laneAdds=${document.querySelectorAll('.lane .lane-add').length}`,
                `boardWarning=${bodyText.includes('No work item states') || bodyText.includes('state')}`,
            ].join('; ');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect new item modal state")?
        .convert::<String>()
        .context("failed to read new item modal state")?)
}

async fn assert_item_detail_description_is_not_duplicated(
    driver: &WebDriver,
) -> Result<(), Report> {
    let summary = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const expected = 'Created through browser-test\nSecond line';
            const deadline = Date.now() + 5000;
            const inspect = () => {
                const headerText = document.querySelector('.item-header')?.textContent ?? '';
                const input = document.querySelector(
                    'section.item-settings input[name="description"]'
                );
                const editor = document.querySelector(
                    'section.item-settings [data-rich-text-field="description"] leptonic-tiptap-editor'
                );
                const descriptionValue = input?.value ?? '';
                const result = [
                    headerText.includes(expected),
                    descriptionValue === expected || (
                        descriptionValue.includes('Created through browser-test') &&
                        descriptionValue.includes('Second line')
                    ),
                    Boolean(editor)
                ].join('|');
                if (result === 'false|true|true' || Date.now() > deadline) {
                    done(result);
                    return;
                }
                setTimeout(inspect, 100);
            };
            inspect();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect item detail description placement")?
        .convert::<String>()
        .context("failed to read item detail description placement")?;
    assert_that!(summary).is_equal_to("false|true|true".to_owned());
    Ok(())
}

async fn assert_item_detail_description_editor_accepts_click_and_text(
    driver: &WebDriver,
) -> Result<(), Report> {
    driver
        .execute(
            r#"window.__dispatchDescriptionEditorClickMarker = 'kept';"#,
            Vec::new(),
        )
        .await
        .context("failed to set description editor click marker")?;

    click_description_editor(driver).await?;
    tokio::time::sleep(Duration::from_millis(250)).await;

    let marker = driver
        .execute(
            r#"
            return window.__dispatchDescriptionEditorClickMarker === 'kept'
                ? 'kept'
                : 'lost';
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect description editor click marker")?
        .convert::<String>()
        .context("failed to read description editor click marker")?;
    assert_that!(marker).is_equal_to("kept".to_owned());

    let editor = find(
        driver,
        By::Css("section.item-settings [data-rich-text-field='description'] .ProseMirror"),
    )
    .await?;
    editor
        .send_keys(" Editable after click")
        .await
        .context("failed to type in description editor after click")?;

    let value = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const deadline = Date.now() + 5000;
            const inspect = () => {
                const input = document.querySelector(
                    'section.item-settings input[name="description"]'
                );
                const value = input?.value ?? '';
                if (value.includes('Editable after click') || Date.now() > deadline) {
                    done(value);
                    return;
                }
                setTimeout(inspect, 100);
            };
            inspect();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect edited description value")?
        .convert::<String>()
        .context("failed to read edited description value")?;
    assert_that!(value).contains("Editable after click");
    Ok(())
}

async fn click_description_editor(driver: &WebDriver) -> Result<(), Report> {
    let selector = "section.item-settings [data-rich-text-field='description'] .ProseMirror";
    let mut last_error = None;
    for _ in 0..5 {
        let editor = find(driver, By::Css(selector)).await?;
        match editor.scroll_into_view().await {
            Ok(()) => match editor.click().await {
                Ok(()) => return Ok(()),
                Err(err) => last_error = Some(format!("click failed: {err}")),
            },
            Err(err) => last_error = Some(format!("scroll failed: {err}")),
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    bail!(
        "failed to click description editor after retries: {}",
        last_error.unwrap_or_else(|| "no click attempt was made".to_owned())
    )
}

async fn assert_item_relationship_create_delete_flow(
    driver: &WebDriver,
    target_id: i64,
) -> Result<(), Report> {
    find(driver, By::Css("section.item-relationships")).await?;
    assert_source_contains(driver, "No relationships").await?;

    let add_script = r#"
        const targetId = TARGET_ID;
        const form = document.querySelector('.relationship-add-form');
        if (!form) {
            throw new Error('missing relationship add form');
        }
        window.__dispatchRelationshipUrl = window.location.href;
        window.__dispatchRelationshipMarker = 'created';
        document.body.style.minHeight = '5000px';
        window.scrollTo(0, 1800);
        const targetInput = form.querySelector('input[name="target_work_item_id"]');
        const kindInput = form.querySelector('input[name="kind"]');
        targetInput.value = String(targetId);
        targetInput.dispatchEvent(new Event('input', { bubbles: true }));
        kindInput.value = 'is follow-up of';
        kindInput.dispatchEvent(new Event('input', { bubbles: true }));
        form.requestSubmit();
    "#
    .replace("TARGET_ID", &target_id.to_string());
    driver
        .execute(add_script, Vec::new())
        .await
        .context("failed to submit relationship add form")?;

    let wait_create_script = r#"
        const targetId = TARGET_ID;
        const done = arguments[0];
        const deadline = Date.now() + 5000;
        const inspect = () => {
            const panel = document.querySelector('section.item-relationships');
            const text = panel?.innerText ?? '';
            const related = panel?.querySelector(`.relationship-related[href="/projects/demo/items/${targetId}"]`);
            const summary = [
                `marker=${window.__dispatchRelationshipMarker ?? '<missing>'}`,
                `sameUrl=${window.location.href === window.__dispatchRelationshipUrl}`,
                `scrollKept=${window.scrollY > 1000}`,
                `hasRow=${Boolean(panel?.querySelector('.relationship-row'))}`,
                `hasRelated=${Boolean(related)}`,
                `hasKind=${text.includes('is follow-up of')}`,
                `hasDirection=${text.includes('outgoing')}`,
                `hasTitle=${text.includes('Relationship target')}`,
            ].join(';');
            if (summary.endsWith('hasTitle=true') || Date.now() > deadline) {
                done(summary);
                return;
            }
            setTimeout(inspect, 100);
        };
        inspect();
    "#
    .replace("TARGET_ID", &target_id.to_string());
    let created_summary = driver
        .execute_async(wait_create_script, Vec::new())
        .await
        .context("failed to wait for relationship row after add")?
        .convert::<String>()
        .context("failed to read relationship add summary")?;
    assert_that!(created_summary).is_equal_to(
        "marker=created;sameUrl=true;scrollKept=true;hasRow=true;hasRelated=true;hasKind=true;hasDirection=true;hasTitle=true"
            .to_owned(),
    );

    driver
        .execute(
            r#"
            const form = document.querySelector('.relationship-kind-form');
            const input = form?.querySelector('input[name="kind"]');
            if (!form || !input) {
                throw new Error('missing relationship update form');
            }
            input.value = 'depends on';
            input.dispatchEvent(new Event('input', { bubbles: true }));
            form.requestSubmit();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to submit relationship update form")?;
    find(driver, By::XPath("//*[contains(text(), 'depends on')]")).await?;

    driver
        .execute(
            r#"
            const form = document.querySelector('.relationship-delete-form');
            if (!form) {
                throw new Error('missing relationship delete form');
            }
            window.__dispatchRelationshipMarker = 'deleted';
            form.requestSubmit();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to submit relationship delete form")?;

    let deleted_summary = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const deadline = Date.now() + 5000;
            const inspect = () => {
                const panel = document.querySelector('section.item-relationships');
                const text = panel?.innerText ?? '';
                const summary = [
                    `marker=${window.__dispatchRelationshipMarker ?? '<missing>'}`,
                    `sameUrl=${window.location.href === window.__dispatchRelationshipUrl}`,
                    `hasRow=${Boolean(panel?.querySelector('.relationship-row'))}`,
                    `empty=${text.includes('No relationships')}`,
                ].join(';');
                if (summary.endsWith('empty=true') || Date.now() > deadline) {
                    document.body.style.minHeight = '';
                    done(summary);
                    return;
                }
                setTimeout(inspect, 100);
            };
            inspect();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to wait for relationship row after delete")?
        .convert::<String>()
        .context("failed to read relationship delete summary")?;
    assert_that!(deleted_summary)
        .is_equal_to("marker=deleted;sameUrl=true;hasRow=false;empty=true".to_owned());
    Ok(())
}

async fn assert_source_contains(driver: &WebDriver, expected: &str) -> Result<(), Report> {
    let source = driver
        .source()
        .await
        .context("failed to read page source")?;
    assert_that!(source).contains(expected);
    Ok(())
}

async fn assert_top_nav_order(driver: &WebDriver) -> Result<(), Report> {
    let labels = driver
        .execute(
            r#"
            return Array.from(document.querySelectorAll('.top-nav a'))
                .map((link) => link.textContent.trim())
                .join('|');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect top navigation")?
        .convert::<String>()
        .context("failed to read top navigation labels")?;
    assert_that!(labels).is_equal_to("Board|Automation|Runs|Projects|API".to_owned());
    Ok(())
}

async fn assert_codex_auth_guide_when_blocked(driver: &WebDriver) -> Result<(), Report> {
    let source = driver
        .source()
        .await
        .context("failed to read page source")?;
    if source.contains("Codex automation blocked") && source.contains("Not signed in") {
        for expected in [
            "Sign in to Codex",
            "CODEX_HOME=",
            "CODEX_SQLITE_HOME=",
            "Copy command",
            "Copy home",
            "Log out",
            "/codex/logout",
            "OPENAI_API_KEY",
        ] {
            if !source.contains(expected) {
                bail!("blocked Codex auth guide did not include {expected:?}");
            }
        }
        if source.contains("Install Codex and make sure") {
            bail!("blocked Codex auth guide unexpectedly included the install prompt");
        }
    }
    Ok(())
}

async fn assert_source_does_not_contain(
    driver: &WebDriver,
    unexpected: &str,
) -> Result<(), Report> {
    let source = driver
        .source()
        .await
        .context("failed to read page source")?;
    if source.contains(unexpected) {
        bail!("page source unexpectedly contained {unexpected:?}");
    }
    Ok(())
}

async fn find(driver: &WebDriver, by: By) -> Result<browser_test::thirtyfour::WebElement, Report> {
    match driver.find(by).await {
        Ok(element) => Ok(element),
        Err(err) => {
            let current_url = driver
                .current_url()
                .await
                .map(|url| url.to_string())
                .unwrap_or_else(|url_err| format!("failed to read current URL: {url_err}"));
            let source = driver
                .source()
                .await
                .unwrap_or_else(|source_err| format!("failed to read page source: {source_err}"));
            let source_prefix = source.chars().take(4_000).collect::<String>();
            bail!(
                "failed to find browser-test element at {current_url}: {err}; source prefix: {source_prefix}"
            );
        }
    }
}

async fn click(driver: &WebDriver, by: By) -> Result<(), Report> {
    let target = format!("{by:?}");
    let element = find(driver, by).await?;
    if let Err(initial_err) = element.click().await {
        element
            .scroll_into_view()
            .await
            .context("failed to scroll browser-test element into view")?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Err(retry_err) = element.click().await {
            bail!(
                "failed to click browser-test element {target}: {retry_err}; initial error: {initial_err}"
            );
        }
    }
    Ok(())
}

async fn submit_label_add_form(driver: &WebDriver) -> Result<(), Report> {
    driver
        .execute(
            r#"
            const form = document.querySelector('.label-add-form');
            if (!form) {
                throw new Error('missing label add form');
            }
            window.__dispatchLabelAddMarker = 'alive';
            window.__dispatchLabelAddUrl = window.location.href;
            document.body.style.minHeight = '5000px';
            window.scrollTo(0, 1600);
            form.requestSubmit();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to submit label add form")?;
    Ok(())
}

async fn assert_label_add_preserved_item_page(driver: &WebDriver) -> Result<(), Report> {
    let summary = driver
        .execute(
            r#"
            const summary = [
                `marker=${window.__dispatchLabelAddMarker ?? '<missing>'}`,
                `sameUrl=${window.location.href === window.__dispatchLabelAddUrl}`,
                `scrollKept=${window.scrollY > 1000}`,
            ].join(';');
            document.body.style.minHeight = '';
            return summary;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect typed item label add")?
        .convert::<String>()
        .context("failed to read typed item label add summary")?;
    assert_that!(summary).is_equal_to("marker=alive;sameUrl=true;scrollKept=true".to_owned());
    Ok(())
}

async fn assert_item_label_update_delete_flow(driver: &WebDriver) -> Result<(), Report> {
    driver
        .execute(
            r#"
            const row = Array.from(document.querySelectorAll('.label-row'))
                .find(row => row.querySelector('.label-chip')?.textContent?.trim() === 'severity=high');
            const form = row?.querySelector('.label-update-form');
            const value = form?.querySelector('input[name="value"]');
            if (!form || !value) {
                throw new Error('missing label update form');
            }
            window.__dispatchLabelMutationUrl = window.location.href;
            value.value = 'critical';
            value.dispatchEvent(new Event('input', { bubbles: true }));
            form.requestSubmit();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to submit typed label update")?;
    find(
        driver,
        By::XPath("//*[contains(text(), 'severity=critical')]"),
    )
    .await?;
    let same_url = driver
        .execute(
            "return window.location.href === window.__dispatchLabelMutationUrl;",
            Vec::new(),
        )
        .await
        .context("failed to inspect typed label update navigation")?
        .convert::<bool>()
        .context("failed to read typed label update navigation result")?;
    assert_that!(same_url).is_true();

    driver
        .execute(
            r#"
            const row = Array.from(document.querySelectorAll('.label-row'))
                .find(row => row.querySelector('.label-chip')?.textContent?.trim() === 'severity=critical');
            const form = row?.querySelector('.label-delete-form');
            if (!form) {
                throw new Error('missing label delete form');
            }
            form.requestSubmit();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to submit typed label delete")?;
    let deleted = driver
        .execute_async(
            r#"
            const done = arguments[0];
            const deadline = Date.now() + 5000;
            const inspect = () => {
                const labels = Array.from(document.querySelectorAll('.label-chip'))
                    .map(label => label.textContent?.trim());
                if (!labels.includes('severity=critical') || Date.now() > deadline) {
                    done(!labels.includes('severity=critical'));
                    return;
                }
                setTimeout(inspect, 100);
            };
            inspect();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to wait for typed label deletion")?
        .convert::<bool>()
        .context("failed to read typed label deletion result")?;
    assert_that!(deleted).is_true();
    Ok(())
}

async fn assert_state_label_dropdown_and_move(driver: &WebDriver) -> Result<(), Report> {
    let summary = driver
        .execute(
            r#"
            const form = document.querySelector('.label-row form.state-label-form');
            if (!form) {
                throw new Error('missing state label form');
            }
            const valueSelect = form.querySelector('select[name="value"]');
            const valueInput = form.querySelector('input[name="value"]');
            if (!valueSelect) {
                throw new Error('missing state label select');
            }
            return [
                `value=${valueSelect.value}`,
                `hasValueInput=${Boolean(valueInput)}`,
                `options=${Array.from(valueSelect.options)
                    .map(option => `${option.value}:${(option.textContent ?? '').trim()}`)
                    .join('|')}`,
            ].join(';');
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect state label select")?
        .convert::<String>()
        .context("failed to read state label select summary")?;
    assert_that!(summary).is_equal_to(
        "value=in_progress;hasValueInput=false;options=idea:Idea|open:Open|in_progress:In progress|done:Done"
            .to_owned(),
    );

    driver
        .execute(
            r#"
            const form = document.querySelector('.label-row form.state-label-form');
            const valueSelect = form?.querySelector('select[name="value"]');
            if (!form || !valueSelect) {
                throw new Error('missing state label form');
            }
            window.__dispatchLabelSaveMarker = 'alive';
            window.__dispatchLabelSaveUrl = window.location.href;
            document.body.style.minHeight = '5000px';
            window.scrollTo(0, 1600);
            valueSelect.value = 'done';
            valueSelect.dispatchEvent(new Event('change', { bubbles: true }));
            form.requestSubmit();
            "#,
            Vec::new(),
        )
        .await
        .context("failed to submit state label move")?;
    find(driver, By::XPath("//*[contains(text(), 'state=done')]")).await?;
    let save_summary = driver
        .execute(
            r#"
            const summary = [
                `marker=${window.__dispatchLabelSaveMarker ?? '<missing>'}`,
                `sameUrl=${window.location.href === window.__dispatchLabelSaveUrl}`,
                `scrollKept=${window.scrollY > 1000}`,
            ].join(';');
            document.body.style.minHeight = '';
            return summary;
            "#,
            Vec::new(),
        )
        .await
        .context("failed to inspect typed item state update")?
        .convert::<String>()
        .context("failed to read typed item state update summary")?;
    assert_that!(save_summary).is_equal_to("marker=alive;sameUrl=true;scrollKept=true".to_owned());
    Ok(())
}

async fn set_input_value(driver: &WebDriver, selector: &str, value: &str) -> Result<(), Report> {
    let script = format!(
        r#"
        const done = arguments[0];
        const selector = {selector:?};
        const value = {value:?};
        const deadline = Date.now() + 5000;
        const editable = (field) => {{
            if (!field) {{
                return null;
            }}
            if (field.matches('input, textarea, select')) {{
                return field;
            }}
            return field.querySelector('input, textarea, select');
        }};
        const findInput = () => {{
            const controls = Array.from(document.querySelectorAll(selector))
                .map(editable)
                .filter(Boolean);
            return controls.find((control) =>
                !control.disabled &&
                !control.readOnly &&
                control.type !== 'hidden'
            ) ?? controls[0] ?? null;
        }};
        const setValue = () => {{
            const input = findInput();
            if (!input) {{
                if (Date.now() > deadline) {{
                    done('missing input ' + selector);
                    return;
                }}
                setTimeout(setValue, 100);
                return;
            }}
            input.value = value;
            input.setAttribute('value', value);
            input.dispatchEvent(new Event('input', {{ bubbles: true }}));
            input.dispatchEvent(new Event('change', {{ bubbles: true }}));
            done('ok');
        }};
        setValue();
        "#
    );
    let result = driver
        .execute_async(script, Vec::new())
        .await
        .context("failed to set browser-test input value")?
        .convert::<String>()
        .context("failed to read browser-test input set result")?;
    if result != "ok" {
        bail!("failed to set browser-test input value: {result}");
    }
    Ok(())
}

async fn send_keys(driver: &WebDriver, by: By, value: &str) -> Result<(), Report> {
    find(driver, by)
        .await?
        .send_keys(value)
        .await
        .context("failed to type into browser-test element")?;
    Ok(())
}
