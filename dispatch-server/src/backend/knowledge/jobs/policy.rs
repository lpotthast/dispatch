use super::model::Record;
use dispatch_types::knowledge::jobs::*;
use rootcause::Result;
use sha2::{Digest, Sha256};
pub(super) fn pass_seconds(record: &Record) -> u64 {
    let remaining = record
        .job()
        .request
        .budget_seconds
        .saturating_sub(record.job().active_millis.div_ceil(1000));
    match record.job().stage {
        JobStage::Discovery => (record.job().request.budget_seconds * 3 / 4)
            .saturating_sub(record.job().active_millis.div_ceil(1000))
            .min(600),
        JobStage::Synthesis => remaining / 2,
        JobStage::Reader => remaining / 2,
        _ => remaining,
    }
    .max(1)
}
pub(super) fn prompt(record: &Record) -> Result<String> {
    let stage = record.job().stage;
    let mut prompt = format!(
        "Knowledge job {}. Stage: {stage:?}. Knowledge directory: {}. This pass has {} seconds; submit a checkpoint before it expires.\nAdditional user context: {}\n",
        record.job().id,
        record.directory,
        pass_seconds(record),
        record.job().request.context
    );
    match stage {
        JobStage::Reader => {
            prompt.push_str("Independently answer these reading questions. Start at the root and navigate using the knowledge CLI. You receive no extraction notes or expected answers. Use source fallback only when knowledge is insufficient. Report summary and answers with this exact JSON shape: {\"summary\":\"...\",\"answers\":[{\"question_id\":\"...\",\"answer\":\"...\",\"references\":[\"README.md#section\"],\"missing\":false}]}. References are strings; missing is a boolean. Explicitly report missing knowledge.\n");
            prompt.push_str(&serde_json::to_string(
                &record
                    .questions
                    .iter()
                    .map(|q| (&q.id, &q.question))
                    .collect::<Vec<_>>(),
            )?);
        }
        JobStage::Review => {
            prompt.push_str("Evaluate the independent answers against the fixed evidence/requirements below. Independently inspect the candidate's changed sections, existing owners, related explanations, and affected summaries for duplicated contracts, misplaced detail, and missing owner links, even when every reading answer is correct. A link beside repeated detail does not resolve duplication. Identify document paths/sections and the owning explanation in review_issues; keep these issues separate from answer correctness. Also check meaningful refinement, unsupported claims, missing exceptions, code transcription, and all affected ancestors. Do not edit files. Report summary, evaluations [{question_id: string, correct: boolean, issues: string[]}], review_issues: string[], findings, and remaining: string[]. Failures require repair or explicit partial review. Compression is meaningful only alongside correctness/unanswered questions; never remove a consequential qualification to reduce reading cost.\n");
            prompt.push_str(&serde_json::to_string(&record.questions)?);
            prompt.push_str(&serde_json::to_string(&record.detail.quality)?);
        }
        _ => {
            prompt.push_str("Reconcile prior discoveries with current source. Before drafting derive representative questions with evidence and an ordinary/exception case. Report summary, areas [{id,responsibility,questions,evidence,aspects,owners,remaining}], questions [{id,question,requirements,evidence:[{path,fingerprint,range:{start,end},content}]}], findings [{id,explanation,references,consequential}], remaining, ready_for_synthesis, complete, reviewed_documents. Synthesis must connect every detail to all affected parents and root, and review dependents; report reviewed IDs including unchanged parents. Complete means no known material coverage gaps, not just process success.\n");
            prompt.push_str(&serde_json::to_string(&record.detail.areas)?);
            prompt.push_str(&serde_json::to_string(&record.detail.findings)?);
            prompt.push_str(&serde_json::to_string(&record.detail.remaining)?);
            prompt.push_str("\nExisting frozen evaluation questions (preserve):\n");
            prompt.push_str(&serde_json::to_string(&record.questions)?);
        }
    }
    Ok(prompt)
}
pub(super) fn charge(record: &mut Record, now: i64) {
    if let Some(start) = record.pass_started_at {
        record.job_mut().active_millis = record
            .job()
            .active_millis
            .saturating_add(now.saturating_sub(start).max(0) as u64);
        record.pass_started_at = Some(now);
    }
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn estimated_tokens(bytes: &[u8]) -> u64 {
    bytes.len().div_ceil(4) as u64
}
pub(crate) fn timestamp_millis() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}
