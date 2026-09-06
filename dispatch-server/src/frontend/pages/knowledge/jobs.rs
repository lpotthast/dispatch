use super::*;
use dispatch_types::knowledge::jobs::*;

#[derive(Clone, Copy)]
pub(super) struct Jobs {
    pub list: RwSignal<Vec<KnowledgeJob>>,
    pub refresh: RwSignal<u64>,
    pub selected: RwSignal<Option<i64>>,
    pub error: RwSignal<Option<String>>,
    pub aspect: RwSignal<String>,
}
impl Jobs {
    pub(super) fn new(
        project: StoredValue<String>,
        service: StoredValue<KnowledgeUiService>,
        workspace_refresh: RwSignal<u64>,
    ) -> Self {
        let query = leptos_router::hooks::use_query_map();
        let jobs = Self {
            list: RwSignal::new(vec![]),
            refresh: RwSignal::new(0),
            selected: RwSignal::new(
                query
                    .get_untracked()
                    .get("job")
                    .and_then(|s| s.parse().ok()),
            ),
            error: RwSignal::new(None),
            aspect: RwSignal::new(String::new()),
        };
        let result = LocalResource::new(move || {
            jobs.refresh.track();
            let service = service.get_value().jobs;
            let project = project.get_value();
            async move { service.list(project).await }
        });
        Effect::new(move || {
            if let Some(result) = result.get() {
                match result {
                    Ok(list) => {
                        let previous = jobs.list.get_untracked();
                        if list.iter().any(|j| {
                            j.outcome.starts_with("Applied")
                                && previous
                                    .iter()
                                    .find(|p| p.id == j.id)
                                    .is_some_and(|p| p.outcome != j.outcome)
                        }) {
                            workspace_refresh.update(|v| *v += 1);
                        }
                        if jobs.selected.get_untracked().is_none() {
                            jobs.selected.set(list.first().map(|j| j.id));
                        }
                        jobs.list.set(list);
                        jobs.error.set(None);
                    }
                    Err(e) => jobs.error.set(Some(error_message(e))),
                }
            }
        });
        #[cfg(target_arch = "wasm32")]
        if let Ok(handle) = set_interval_with_handle(
            move || jobs.refresh.update(|v| *v += 1),
            std::time::Duration::from_secs(3),
        ) {
            on_cleanup(move || handle.clear());
        }
        jobs
    }
}
#[component]
pub(super) fn Activity(
    jobs: Jobs,
    open: Callback<Tool>,
    initialized: Signal<bool>,
) -> impl IntoView {
    view! {<section class="knowledge-activity" aria-label="Knowledge activity">
        <div role="status">{move || jobs.list.get().first().map(|j|format!("Job #{} · {:?} · {:?} · {}",j.id,j.status,j.stage,j.progress)).unwrap_or_else(||"Discover the project's important behavior and decisions.".into())}</div>
        <button type="button" on:click=move |_|open.run(Tool::Jobs)>{move ||if jobs.list.get().iter().any(|j|j.status.unresolved()) {"View knowledge job"}else if initialized.get() || !jobs.list.get().is_empty() {"Continue discovery"}else{"Initialize knowledge"}}</button>
    </section>}
}
#[component]
pub(super) fn JobsDrawer(
    project: String,
    service: KnowledgeUiService,
    jobs: Jobs,
    initialized: bool,
    select: Callback<String>,
) -> impl IntoView {
    let project = StoredValue::new(project);
    let owner_service = StoredValue::new(service.clone());
    let owner_select = Callback::new(move |id: String| {
        let service = owner_service.get_value();
        let project = project.get_value();
        leptos::task::spawn_local(async move {
            match service
                .read(
                    project,
                    KnowledgeOperation::Node,
                    KnowledgeQuery {
                        id: Some(id),
                        ..Default::default()
                    },
                )
                .await
            {
                Ok(view) => {
                    if let Some(doc) = view.document {
                        select.run(doc.summary.path);
                    }
                }
                Err(e) => jobs.error.set(Some(error_message(e))),
            }
        });
    });
    let service = StoredValue::new(service.jobs);
    let context = RwSignal::new(String::new());
    let budget = RwSignal::new(3600_u64);
    let token_budget = RwSignal::new(String::new());
    let request_id = RwSignal::new(uuid::Uuid::new_v4().to_string());
    let pending = RwSignal::new(false);
    let application = RwSignal::new(ApplicationMode::Review);
    let settings = LocalResource::new(move || {
        let service = service.get_value();
        let project = project.get_value();
        async move { service.settings(project).await }
    });
    Effect::new(move || {
        if let Some(Ok(settings)) = settings.get() {
            application.set(settings.application_mode);
        }
    });
    let save_mode = move |event| {
        let mode = if event_target_value(&event) == "automatic" {
            ApplicationMode::Automatic
        } else {
            ApplicationMode::Review
        };
        let service = service.get_value();
        let project = project.get_value();
        leptos::task::spawn_local(async move {
            match service
                .save_settings(
                    project,
                    KnowledgeSettings {
                        application_mode: mode,
                    },
                )
                .await
            {
                Ok(s) => application.set(s.application_mode),
                Err(e) => jobs.error.set(Some(error_message(e))),
            }
        });
    };
    let action = Callback::new(move |(id, action): (i64, JobAction)| {
        if pending.get_untracked() {
            return;
        }
        pending.set(true);
        let service = service.get_value();
        let project = project.get_value();
        leptos::task::spawn_local(async move {
            match service.action(project, id, action).await {
                Ok(job) => {
                    jobs.selected.set(Some(job.id));
                    jobs.refresh.update(|v| *v += 1);
                    jobs.error.set(None);
                }
                Err(e) => jobs.error.set(Some(error_message(e))),
            }
            pending.set(false);
        });
    });
    let start = move |_| {
        if pending.get_untracked() {
            return;
        }
        let token_budget = match token_budget.get_untracked().trim() {
            "" => None,
            value => match value.parse::<u64>() {
                Ok(value) => Some(value),
                Err(_) => {
                    jobs.error
                        .set(Some("Token budget must be a positive integer".into()));
                    return;
                }
            },
        };
        pending.set(true);
        let request = StartKnowledgeJob {
            request_id: request_id.get_untracked(),
            context: context.get_untracked(),
            budget_seconds: budget.get_untracked(),
            token_budget,
            previous_job_id: jobs.list.get_untracked().first().map(|j| j.id),
            ..Default::default()
        };
        let service = service.get_value();
        let project = project.get_value();
        leptos::task::spawn_local(async move {
            match service.start(project, request).await {
                Ok(job) => {
                    jobs.selected.set(Some(job.id));
                    jobs.refresh.update(|v| *v += 1);
                    jobs.error.set(None);
                    request_id.set(uuid::Uuid::new_v4().to_string());
                }
                Err(e) => jobs.error.set(Some(error_message(e))),
            }
            pending.set(false);
        });
    };
    // Refresh checkpoint content on pass/outcome changes. Heartbeats update the activity strip
    // without replacing focused inputs and expanded sections in the drawer every three seconds.
    let detail_key = Memo::new(move |_| {
        let id = jobs.selected.get();
        let checkpoint = jobs.list.with(|list| {
            list.iter().find(|j| Some(j.id) == id).map(|j| {
                (
                    j.status,
                    j.stage,
                    j.active_run_id,
                    j.run_ids.len(),
                    j.recovery_attempts,
                    j.outcome.clone(),
                )
            })
        });
        (id, checkpoint)
    });
    let detail = LocalResource::new(move || {
        let id = detail_key.get().0;
        let service = service.get_value();
        let project = project.get_value();
        async move {
            match id {
                Some(id) => service.detail(project, id).await.map(Some),
                None => Ok(None),
            }
        }
    });
    view! {<div class="knowledge-jobs">
        <label>"Changes to accepted documents"<select aria-label="Knowledge application mode" prop:value=move ||if application.get()==ApplicationMode::Automatic{"automatic"}else{"review"} on:change=save_mode><option value="review">"Review proposals"</option><option value="automatic">"Apply validated changes automatically"</option></select></label>
        <p>"Discovery runs for up to one hour of active agent work by default. Continue discovery authorizes another bounded run. Recurring automation is disabled."</p>
        {move ||jobs.error.get().map(|e|view!{<p class="callout danger" role="alert">{e}</p>})}
        <Show when=move ||!jobs.list.get().iter().any(|j|j.status.unresolved())>
            <label>"Additional context or documentation links"<textarea prop:value=move ||context.get() on:input=move |e|context.set(event_target_value(&e))/></label>
            <details><summary>"Advanced runtime limits"</summary>
                <label>"Active work (seconds)"<input type="number" min="60" max="86400" prop:value=move ||budget.get().to_string() on:input=move |e|if let Ok(v)=event_target_value(&e).parse(){budget.set(v);}/></label>
                <label>"Reported-token budget (optional)"<input type="number" min="1" prop:value=move ||token_budget.get() on:input=move |e|token_budget.set(event_target_value(&e))/></label>
                <p>"Usage is reported by the provider. An in-flight turn may exceed the token budget; without usage, only the time limit applies."</p>
            </details>
            <button type="button" disabled=move ||pending.get() on:click=start>{move ||if initialized || !jobs.list.get().is_empty(){"Continue discovery"}else{"Initialize knowledge"}}</button>
        </Show>
        <button type="button" on:click=move |_|detail.refetch()>"Refresh job details"</button>
        <ul class="knowledge-job-list">{move ||jobs.list.get().into_iter().map(|job|{let id=job.id;view!{<li><button type="button" aria-pressed=move ||(jobs.selected.get()==Some(id)).to_string() on:click=move |_|jobs.selected.set(Some(id))>{format!("#{} · {:?} · {:?}",id,job.status,job.stage)}</button></li>}}).collect_view()}</ul>
        {move ||detail.get().map(|result|match result {
            Ok(Some(detail))=>view!{<JobDetail detail project=project.get_value() service=service.get_value() pending action select owner_select jobs/>}.into_any(),
            Ok(None)=>().into_any(),Err(e)=>view!{<p role="alert">{error_message(e)}</p>}.into_any(),
        })}
    </div>}
}
#[component]
fn JobDetail(
    detail: KnowledgeJobDetail,
    project: String,
    service: crate::frontend::services::KnowledgeJobsUiService,
    pending: RwSignal<bool>,
    action: Callback<(i64, JobAction)>,
    select: Callback<String>,
    owner_select: Callback<String>,
    jobs: Jobs,
) -> impl IntoView {
    let Some(job) = detail.job else {
        return ().into_any();
    };
    let id = job.id;
    let status = job.status;
    let aspect = jobs.aspect;
    let project = StoredValue::new(project);
    let service = StoredValue::new(service);
    let coverage = LocalResource::new(move || {
        let aspect = aspect.get();
        let service = service.get_value();
        let project = project.get_value();
        async move {
            service
                .coverage(project, id, (!aspect.is_empty()).then_some(aspect))
                .await
        }
    });
    let quality = detail.quality;
    let correct = quality
        .evaluations
        .iter()
        .filter(|e| e.correct && e.issues.is_empty())
        .count();
    let missing = quality.answers.iter().filter(|a| a.missing).count();
    view!{<section class="knowledge-job-detail">
        <h3>{format!("Job #{id}")}</h3><p>{job.outcome}</p><p>{move ||jobs.list.with(|list|list.iter().find(|j|j.id==id).map(|j|format!("Active work: {} / {} seconds · Recovery attempts: {} / 3",j.active_millis/1000,j.request.budget_seconds,j.recovery_attempts)).unwrap_or_default())}</p>
        <p>{move ||jobs.list.with(|list|list.iter().find(|j|j.id==id).map(|j|format!("Area: {} · Aspect: {}",j.current_area.as_deref().unwrap_or_default(),j.current_aspect.as_deref().unwrap_or_default())).unwrap_or_default())}</p>
        <div class="knowledge-job-actions">
            {if matches!(status,JobStatus::Queued|JobStatus::Running) {Some(view!{<button type="button" disabled=move ||pending.get() on:click=move |_|action.run((id,JobAction::Cancel))>"Cancel job"</button>})}else{None}}
            {if status==JobStatus::AwaitingReview {Some(view!{<button type="button" disabled=move ||pending.get() on:click=move |_|action.run((id,JobAction::Apply))>"Apply candidate"</button><button type="button" disabled=move ||pending.get() on:click=move |_|action.run((id,JobAction::Reject))>"Reject candidate"</button>})}else{None}}
            {if matches!(status,JobStatus::Failed|JobStatus::Cancelled) {Some(view!{<button type="button" disabled=move ||pending.get() on:click=move |_|action.run((id,JobAction::Retry{request_id:uuid::Uuid::new_v4().to_string()}))>"Retry"</button>})}else{None}}
        </div>
        <h4>"Agent runs"</h4><ul>{job.run_ids.into_iter().map(|run|view!{<li><a href=format!("/projects/{}/automation/runs/{run}/log",urlencoding::encode(&project.get_value()))>{format!("Run #{run} · logs, instructions and usage")}</a></li>}).collect_view()}</ul>
        <h4>"Discovery map and remaining scope"</h4>{detail.areas.into_iter().map(|a|view!{<details><summary>{a.responsibility}</summary><p>{a.aspects.join(", ")}</p><ul>{a.questions.into_iter().chain(a.remaining).map(|q|view!{<li>{q}</li>}).collect_view()}</ul></details>}).collect_view()}
        <ul>{detail.remaining.into_iter().chain(detail.validation).map(|v|view!{<li>{v}</li>}).collect_view()}</ul>
        <h4>"Reading quality"</h4><p>{format!("Correct: {correct} / {} · Unanswered: {missing}",quality.answers.len())}</p>
        <p>{format!("Reading: {} estimated tokens · Source fallback: {} · Evidence baseline: {}",quality.reading_estimated_tokens,quality.source_fallback_estimated_tokens,quality.evidence_estimated_tokens)}</p><p>"Estimated tokens use UTF-8 bytes ÷ 4, rounded up. Compare only the same question set and evidence baseline, alongside correctness."</p><code>{quality.question_set_fingerprint}</code>
        {quality.answers.into_iter().map(|answer|view!{<details><summary>{answer.question_id}</summary><p>{answer.answer}</p><p>{answer.references.join(", ")}</p></details>}).collect_view()}
        {quality.evaluations.into_iter().map(|e|view!{<p>{format!("{}: {}",e.question_id,e.issues.join("; "))}</p>}).collect_view()}
        <h4>"Findings"</h4>{detail.findings.into_iter().map(|f|view!{<p>{format!("{}: {}",f.id,f.explanation)}</p>}).collect_view()}
        <h4>"Candidate documents"</h4>{detail.changes.into_iter().map(|c|{let path=c.path.clone();view!{<details><summary>{c.path}</summary><button type="button" on:click=move |_|select.run(path.clone())>"Open accepted document"</button><h5>"Before"</h5><pre>{c.before.unwrap_or_default()}</pre><h5>"Candidate"</h5><pre>{c.after.unwrap_or_default()}</pre></details>}}).collect_view()}
        <h4>"Coverage"</h4><label>"Aspect"<input placeholder="All aspects" prop:value=move ||aspect.get() on:input=move |e|aspect.set(event_target_value(&e))/></label>
        {move ||coverage.get().map(|r|match r {Ok(c)=>view!{
            <p>{format!("Supplied: {} lines · Considered for at least one aspect: {} / {} lines · Accounted for: {}",c.supplied_lines,c.considered_lines,c.total_lines,c.accounted_lines)}</p>
            <p>{format!("Known aspects: {}",c.aspects.join(", "))}</p>
            <details><summary>"Assessment history and knowledge owners"</summary><ul>{c.assessments.into_iter().map(|a|view!{<li><code>{a.assessment.path}</code>{format!(" · {} · {:?} · {:?} · job #{} / run #{}",a.assessment.aspect,a.assessment.disposition,a.assessment.ranges,a.job_id,a.run_id)}<p>{a.assessment.explanation}</p>{a.assessment.document_ids.into_iter().map(|id|{let label=id.clone();view!{<button type="button" on:click=move |_|owner_select.run(id.clone())>{label}</button>}}).collect_view()}</li>}).collect_view()}</ul></details>
            <ul>{c.investigation.into_iter().map(|r|view!{<li><code>{r.path}</code>{format!(" {:?} · {} · {}",r.ranges,r.aspect.unwrap_or_default(),r.reason)}{r.document_ids.into_iter().map(|id|{let label=id.clone();view!{<button type="button" on:click=move |_|owner_select.run(id.clone())>{label}</button>}}).collect_view()}</li>}).collect_view()}</ul>
        }.into_any(),Err(e)=>view!{<p role="alert">{error_message(e)}</p>}.into_any()})}
    </section>}.into_any()
}
