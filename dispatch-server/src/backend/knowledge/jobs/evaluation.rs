//! Frozen reading questions and independent reader/reviewer results.
use super::*;
use std::collections::BTreeSet;

pub(super) fn accept_report(record: &mut Record, run: i64, report: JobReport) -> Result<()> {
    if report.summary.trim().is_empty() {
        bail!("job report requires a summary");
    }
    match record.job().stage {
        JobStage::Discovery => {
            for question in &report.questions {
                if question.id.trim().is_empty()
                    || question.question.trim().is_empty()
                    || question.requirements.is_empty()
                    || question.evidence.is_empty()
                {
                    bail!(
                        "reading questions require stable IDs, expected requirements, and retained evidence"
                    );
                }
                if let Some(old) = record.questions.iter().find(|q| q.id == question.id) {
                    if old != question {
                        bail!(
                            "existing evaluation questions and evidence baselines are immutable; use a new ID for a different question"
                        );
                    }
                    continue;
                }
                for evidence in &question.evidence {
                    sources::permitted(record, &evidence.path)?;
                    let supplied = record.supplied.iter().any(|s| {
                        s.path == evidence.path
                            && s.fingerprint == evidence.fingerprint
                            && s.range.start <= evidence.range.start
                            && s.range.end >= evidence.range.end
                    });
                    if !supplied {
                        bail!("question evidence was not supplied to this job");
                    }
                    let body = fs::read_to_string(record.inputs().join(&evidence.path))?;
                    let actual = body
                        .lines()
                        .skip(evidence.range.start - 1)
                        .take(evidence.range.end - evidence.range.start + 1)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if actual != evidence.content {
                        bail!("question evidence does not match retained source");
                    }
                }
                record.questions.push(question.clone());
            }
            for area in &report.areas {
                if area.id.trim().is_empty() || area.responsibility.trim().is_empty() {
                    bail!("discovery areas require identity and responsibility");
                }
                if let Some(old) = record.detail.areas.iter_mut().find(|a| a.id == area.id) {
                    *old = area.clone();
                } else {
                    record.detail.areas.push(area.clone());
                }
            }
        }
        JobStage::Synthesis => {
            record.reviewed_documents = report.reviewed_documents.clone();
            record.complete = report.complete;
        }
        JobStage::Reader => {
            if !report.questions.is_empty() || !report.evaluations.is_empty() {
                bail!("reader submits answers without expected-answer access");
            }
            let ids: BTreeSet<_> = report
                .answers
                .iter()
                .map(|a| a.question_id.as_str())
                .collect();
            if ids.len() != report.answers.len()
                || record
                    .questions
                    .iter()
                    .any(|q| !ids.contains(q.id.as_str()))
            {
                bail!("reader must answer every frozen question or identify it as missing");
            }
            record.detail.quality.answers = report.answers.clone();
        }
        JobStage::Review => {
            let ids: BTreeSet<_> = report
                .evaluations
                .iter()
                .map(|a| a.question_id.as_str())
                .collect();
            if ids.len() != report.evaluations.len()
                || record
                    .questions
                    .iter()
                    .any(|q| !ids.contains(q.id.as_str()))
            {
                bail!("review must evaluate every frozen reading question");
            }
            record.detail.quality.evaluations = report.evaluations.clone();
            record.detail.quality.review_issues = report.review_issues.clone();
        }
        _ => bail!("this job stage does not accept agent reports"),
    }
    if record.job().stage != JobStage::Reader {
        for finding in &report.findings {
            if let Some(old) = record
                .detail
                .findings
                .iter_mut()
                .find(|f| f.id == finding.id)
            {
                *old = finding.clone();
            } else {
                record.detail.findings.push(finding.clone());
            }
        }
        if matches!(
            record.job().stage,
            JobStage::Discovery | JobStage::Synthesis
        ) {
            record.detail.remaining = report.remaining.clone();
        } else {
            for gap in &report.remaining {
                if !record.detail.remaining.contains(gap) {
                    record.detail.remaining.push(gap.clone());
                }
            }
        }
    }
    record.reports.push((run, report));
    Ok(())
}
pub(super) fn freeze_questions(record: &mut Record) -> Result<()> {
    record.detail.quality.question_set_fingerprint = hash(&serde_json::to_vec(&record.questions)?);
    let mut packages = std::collections::BTreeMap::new();
    for q in &record.questions {
        for e in &q.evidence {
            packages.insert(
                (
                    e.path.clone(),
                    e.fingerprint.clone(),
                    e.range.start,
                    e.range.end,
                ),
                e.content.clone(),
            );
        }
    }
    record.detail.quality.evidence_estimated_tokens = packages
        .values()
        .map(|s| estimated_tokens(s.as_bytes()))
        .sum();
    Ok(())
}
pub(super) fn quality_issues(record: &Record) -> Vec<String> {
    let mut issues = record.detail.quality.review_issues.clone();
    let current = sources::current_assessments(record);
    if current.is_empty() {
        issues.push("No source aspects were assessed".into());
    }
    for area in &record.detail.areas {
        for aspect in &area.aspects {
            if !current.iter().any(|a| {
                a.assessment.aspect == *aspect
                    && a.assessment.disposition != Disposition::FurtherAnalysis
            }) {
                issues.push(format!("Known aspect '{aspect}' has no accounted evidence"));
            }
        }
    }
    if record.questions.is_empty() {
        issues.push("No representative reading questions were evaluated".into());
    }
    if !record.complete
        || !record.detail.remaining.is_empty()
        || record.detail.areas.iter().any(|a| !a.remaining.is_empty())
    {
        issues.push("Partial discovery requires explicit review".into());
    }
    if record.detail.findings.iter().any(|f| f.consequential) {
        issues.push("Unresolved consequential findings require review".into());
    }
    for question in &record.questions {
        if !record
            .detail
            .quality
            .evaluations
            .iter()
            .any(|e| e.question_id == question.id && e.correct && e.issues.is_empty())
        {
            issues.push(format!(
                "Reading question '{}' did not pass independent review",
                question.id
            ));
        }
        if record
            .detail
            .quality
            .answers
            .iter()
            .find(|a| a.question_id == question.id)
            .is_none_or(|a| a.missing || a.references.is_empty())
        {
            issues.push(format!(
                "Reading question '{}' is unanswered or lacks references",
                question.id
            ));
        }
    }
    issues
}
