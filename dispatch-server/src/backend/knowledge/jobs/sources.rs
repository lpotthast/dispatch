//! Retained, exclusion-scoped evidence and aspect-specific consideration.
use super::*;
use crate::backend::knowledge::discovery::Discovery;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn capture(
    workspace: &Path,
    directory: &str,
    destination: &Path,
) -> Result<Vec<SourceFile>> {
    let inventory = Discovery::sources(workspace);
    if !inventory.diagnostics.is_empty() {
        bail!(
            "source inventory has unreadable scope: {:?}",
            inventory.diagnostics
        );
    }
    let mut files = Vec::new();
    let mut total = 0_usize;
    let mut omissions = Vec::new();
    for path in inventory.files {
        let relative = path.strip_prefix(workspace)?.to_string_lossy().into_owned();
        if fs::metadata(&path)?.len() > 4 * 1024 * 1024 {
            omissions.push(format!("{relative}: exceeds the 4 MiB source-input limit"));
            continue;
        }
        let bytes = fs::read(&path)?;
        let Ok(body) = String::from_utf8(bytes) else {
            omissions.push(format!("{relative}: non-UTF-8 input"));
            continue;
        };
        if body.contains('\0') {
            omissions.push(format!("{relative}: binary input"));
            continue;
        }
        total += body.len();
        if total > 256 * 1024 * 1024 {
            bail!("source input exceeds 256 MiB; narrow participation with .dispatchignore");
        }
        let info = SourceFile {
            path: relative.clone(),
            fingerprint: hash(body.as_bytes()),
            lines: body.lines().count(),
            bytes: body.len(),
        };
        // Both sides are compared after copying; a dirty checkout is valid, a torn copy is not.
        let target = destination.join(&relative);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(&target, &body)?;
        files.push(info);
    }
    // Controls are freshness inputs too, but never source retrieval entries.
    let controls: Vec<_> = inventory
        .controls
        .into_iter()
        .map(|(p, h)| {
            (
                p.strip_prefix(workspace)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                h,
            )
        })
        .collect();
    atomic_json(
        &destination.parent().unwrap().join("controls.json"),
        &controls,
    )?;
    atomic_json(
        &destination.parent().unwrap().join("source-omissions.json"),
        &omissions,
    )?;
    check_fresh(workspace, directory, destination, &files)?;
    Ok(files)
}

pub(super) fn check_fresh(
    workspace: &Path,
    _directory: &str,
    inputs: &Path,
    files: &[SourceFile],
) -> Result<()> {
    let inventory = Discovery::sources(workspace);
    if !inventory.diagnostics.is_empty() {
        bail!("source scope cannot be revalidated");
    }
    let controls: Vec<(String, String)> =
        serde_json::from_slice(&fs::read(inputs.parent().unwrap().join("controls.json"))?)?;
    let current_controls: Vec<_> = inventory
        .controls
        .into_iter()
        .map(|(p, h)| {
            (
                p.strip_prefix(workspace)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                h,
            )
        })
        .collect();
    if controls != current_controls {
        bail!("ignore controls changed; this candidate is stale");
    }
    // Conservatively invalidate the full input scope when independence is not established.
    let mut current = Vec::new();
    for path in inventory.files {
        if fs::metadata(&path)?.len() > 4 * 1024 * 1024 {
            continue;
        }
        let bytes = fs::read(&path)?;
        let Ok(body) = String::from_utf8(bytes) else {
            continue;
        };
        if body.contains('\0') {
            continue;
        }
        current.push((
            path.strip_prefix(workspace)?.to_string_lossy().into_owned(),
            hash(body.as_bytes()),
        ));
    }
    let expected: Vec<_> = files
        .iter()
        .map(|f| (f.path.clone(), f.fingerprint.clone()))
        .collect();
    if current != expected {
        bail!("source or knowledge changed since capture; this candidate is stale");
    }
    Ok(())
}

pub(super) fn permitted(record: &Record, path: &str) -> Result<()> {
    super::super::normalize_knowledge_directory(path)?;
    Discovery::check_file(Path::new(&record.workspace), "", path)?;
    if !record.files.iter().any(|f| f.path == path) {
        bail!("path is not retained in this job's permitted source inventory");
    }
    Ok(())
}

pub(super) fn query(
    record: &mut Record,
    run_id: i64,
    operation: &str,
    query: SourceQuery,
) -> Result<SourceResponse> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let mut response = SourceResponse::default();
    match operation {
        "list" => {
            let files: Vec<_> = record
                .files
                .iter()
                .filter(|f| query.path.as_ref().is_none_or(|p| f.path.starts_with(p)))
                .collect();
            response.next_offset = (files.len() > query.offset.saturating_add(limit))
                .then_some(query.offset.saturating_add(limit));
            response.files = files
                .into_iter()
                .skip(query.offset)
                .take(limit)
                .cloned()
                .collect();
            for file in &response.files {
                permitted(record, &file.path)?;
            }
        }
        "read" => {
            let path = query
                .path
                .as_ref()
                .ok_or_else(|| report!("source read requires path"))?;
            permitted(record, path)?;
            let file = record.files.iter().find(|f| &f.path == path).unwrap();
            let start = query.start.unwrap_or(1);
            let end = query
                .end
                .unwrap_or(start.saturating_add(199))
                .min(file.lines);
            if start == 0 || end < start || end - start >= 1000 {
                bail!("source ranges are 1-based inclusive, at most 1000 lines");
            }
            let body =
                fs::read_to_string(Path::new(&record.artifact_dir).join("inputs").join(path))?;
            if hash(body.as_bytes()) != file.fingerprint {
                bail!("retained source fingerprint changed");
            }
            let mut lines = Vec::new();
            let mut bytes = 0;
            for line in body.lines().skip(start - 1).take(end - start + 1) {
                if bytes + line.len() > 32_000 {
                    break;
                }
                bytes += line.len() + 1;
                lines.push(line);
            }
            if lines.is_empty() {
                bail!("source line exceeds the 32,000-byte response limit");
            }
            let actual_end = start + lines.len() - 1;
            response.next_line = (actual_end < end).then_some(actual_end + 1);
            response.excerpts.push(SourceExcerpt {
                path: path.clone(),
                fingerprint: file.fingerprint.clone(),
                range: LineRange {
                    start,
                    end: actual_end,
                },
                content: lines.join("\n"),
            });
        }
        "search" => {
            let text = query
                .text
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| report!("source search requires text"))?;
            let mut matches = Vec::new();
            for file in record
                .files
                .iter()
                .filter(|f| query.path.as_ref().is_none_or(|p| f.path.starts_with(p)))
            {
                permitted(record, &file.path)?;
                let body = fs::read_to_string(
                    Path::new(&record.artifact_dir)
                        .join("inputs")
                        .join(&file.path),
                )?;
                if hash(body.as_bytes()) != file.fingerprint {
                    bail!("retained source fingerprint changed");
                }
                for (i, line) in body
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.contains(text))
                {
                    // A truncated search hit cannot account for the whole line.
                    if line.len() > 2000 {
                        continue;
                    }
                    matches.push(SourceExcerpt {
                        path: file.path.clone(),
                        fingerprint: file.fingerprint.clone(),
                        range: LineRange {
                            start: i + 1,
                            end: i + 1,
                        },
                        content: line.into(),
                    });
                }
            }
            response.next_offset = (matches.len() > query.offset.saturating_add(limit))
                .then_some(query.offset.saturating_add(limit));
            response.excerpts = matches.into_iter().skip(query.offset).take(limit).collect();
        }
        _ => bail!("unknown source operation"),
    }
    for excerpt in &response.excerpts {
        record.supplied.push(Supplied {
            run_id,
            path: excerpt.path.clone(),
            fingerprint: excerpt.fingerprint.clone(),
            range: excerpt.range,
        });
    }
    if record.job().stage == JobStage::Reader {
        let cost = estimated_tokens(serde_json::to_string(&response)?.as_bytes());
        record.detail.quality.reading_estimated_tokens += cost;
        record.detail.quality.source_fallback_estimated_tokens += cost;
    }
    Ok(response)
}

pub(super) fn assess(record: &mut Record, run_id: i64, assessment: AspectAssessment) -> Result<()> {
    permitted(record, &assessment.path)?;
    if assessment.aspect.trim().is_empty()
        || assessment.explanation.trim().is_empty()
        || assessment.ranges.is_empty()
    {
        bail!("an assessment requires an aspect, ranges, and explanation");
    }
    let file = record
        .files
        .iter()
        .find(|f| f.path == assessment.path)
        .unwrap();
    if assessment.fingerprint != file.fingerprint {
        bail!("assessment fingerprint differs from retained evidence");
    }
    for range in &assessment.ranges {
        if range.start == 0 || range.end < range.start || range.end > file.lines {
            bail!("invalid assessment line range");
        }
        let supplied: Vec<_> = record
            .supplied
            .iter()
            .filter(|s| s.path == assessment.path && s.fingerprint == assessment.fingerprint)
            .map(|s| s.range)
            .collect();
        if !(range.start..=range.end)
            .all(|line| supplied.iter().any(|r| r.start <= line && line <= r.end))
        {
            bail!("assessment includes lines that were not supplied through source retrieval");
        }
    }
    match assessment.disposition {
        Disposition::Represented if assessment.document_ids.is_empty() => {
            bail!("represented assessments require document IDs")
        }
        Disposition::Unresolved if assessment.finding_ids.is_empty() => {
            bail!("unresolved assessments require finding IDs")
        }
        _ => {}
    }
    let item = RecordedAssessment {
        job_id: record.job().id,
        run_id,
        assessment,
    };
    if !record.detail.assessments.contains(&item) {
        record.detail.assessments.push(item);
    }
    Ok(())
}

/// Historical assessments remain inspectable; newer dispositions replace the same evidence/aspect.
pub(super) fn current_assessments(record: &Record) -> Vec<&RecordedAssessment> {
    let mut seen = BTreeSet::new();
    record
        .detail
        .assessments
        .iter()
        .rev()
        .filter(|a| {
            let a = &a.assessment;
            record
                .files
                .iter()
                .any(|f| f.path == a.path && f.fingerprint == a.fingerprint)
                && seen.insert((
                    a.path.clone(),
                    a.aspect.clone(),
                    a.ranges
                        .iter()
                        .map(|r| (r.start, r.end))
                        .collect::<Vec<_>>(),
                ))
        })
        .collect()
}

pub(super) fn coverage(record: &Record, aspect: Option<&str>) -> Result<CoverageView> {
    let inventory = Discovery::sources(Path::new(&record.workspace));
    if !inventory.diagnostics.is_empty() {
        bail!("cannot inspect current source scope");
    }
    let mut current = BTreeMap::new();
    for path in inventory.files {
        if fs::metadata(&path)?.len() > 4 * 1024 * 1024 {
            continue;
        }
        let Ok(body) = fs::read_to_string(&path) else {
            continue;
        };
        if body.contains('\0') {
            continue;
        }
        let path = path
            .strip_prefix(&record.workspace)?
            .to_string_lossy()
            .into_owned();
        current.insert(
            path.clone(),
            (
                SourceFile {
                    path,
                    fingerprint: hash(body.as_bytes()),
                    lines: body.lines().count(),
                    bytes: body.len(),
                },
                body,
            ),
        );
    }
    let mut view = CoverageView::default();
    let mut aspects = BTreeSet::new();
    for area in &record.detail.areas {
        aspects.extend(area.aspects.iter().cloned());
    }
    for a in &record.detail.assessments {
        aspects.insert(a.assessment.aspect.clone());
    }
    view.aspects = aspects.into_iter().collect();
    for (path, (file, body)) in &current {
        let baseline = record.job().request.previous_job_id.map(|id| {
            Path::new(&record.artifact_dir)
                .parent()
                .unwrap()
                .join(id.to_string())
                .join("inputs")
                .join(path)
        });
        let baseline = baseline
            .as_deref()
            .unwrap_or(&record.inputs().join(path))
            .to_path_buf();
        let old = fs::read_to_string(&baseline).unwrap_or_default();
        if hash(old.as_bytes()) != file.fingerprint {
            view.investigation.extend(changed_hunks(path, &old, body)?);
        }
        let assessments: Vec<_> = record
            .detail
            .assessments
            .iter()
            .filter(|a| {
                a.assessment.path == *path && aspect.is_none_or(|s| s == a.assessment.aspect)
            })
            .collect();
        let mut seen = BTreeSet::new();
        let valid: Vec<_> = assessments
            .iter()
            .rev()
            .filter(|a| {
                a.assessment.fingerprint == file.fingerprint
                    && seen.insert((
                        a.assessment.aspect.clone(),
                        a.assessment
                            .ranges
                            .iter()
                            .map(|r| (r.start, r.end))
                            .collect::<Vec<_>>(),
                    ))
            })
            .collect();
        let mut considered = BTreeSet::new();
        let mut accounted = BTreeSet::new();
        let mut supplied = BTreeSet::new();
        for a in &valid {
            for r in &a.assessment.ranges {
                considered.extend(r.start..=r.end);
                if a.assessment.disposition != Disposition::FurtherAnalysis {
                    accounted.extend(r.start..=r.end);
                }
            }
        }
        for s in record
            .supplied
            .iter()
            .filter(|s| s.path == *path && s.fingerprint == file.fingerprint)
        {
            supplied.extend(s.range.start..=s.range.end);
        }
        view.supplied_lines += supplied.len();
        view.considered_lines += considered.len();
        view.accounted_lines += accounted.len();
        view.total_lines += file.lines;
        let missing = ranges((1..=file.lines).filter(|i| !considered.contains(i)));
        if !missing.is_empty() {
            view.investigation.push(CoverageRegion {
                path: path.clone(),
                ranges: missing,
                aspect: aspect.map(str::to_owned),
                reason: "not considered for this aspect on current evidence".into(),
                document_ids: vec![],
            });
        }
        for a in &assessments {
            if !valid.contains(&a)
                && valid
                    .iter()
                    .any(|new| new.assessment.aspect == a.assessment.aspect)
            {
                continue;
            }
            if a.assessment.fingerprint != file.fingerprint
                || a.assessment.disposition == Disposition::FurtherAnalysis
                || a.assessment.disposition == Disposition::Unresolved
            {
                view.investigation.push(CoverageRegion {
                    path: path.clone(),
                    ranges: a.assessment.ranges.clone(),
                    aspect: Some(a.assessment.aspect.clone()),
                    reason: if a.assessment.fingerprint != file.fingerprint {
                        "changed evidence; reconsider this aspect"
                    } else {
                        "outstanding aspect"
                    }
                    .into(),
                    document_ids: a.assessment.document_ids.clone(),
                });
            }
        }
    }
    for a in &record.detail.assessments {
        if !current.contains_key(&a.assessment.path)
            && aspect.is_none_or(|s| s == a.assessment.aspect)
        {
            view.investigation.push(CoverageRegion {
                path: a.assessment.path.clone(),
                ranges: vec![],
                aspect: Some(a.assessment.aspect.clone()),
                reason: "deleted or excluded evidence; reconsider associated knowledge".into(),
                document_ids: a.assessment.document_ids.clone(),
            });
        }
    }
    view.files = current.into_values().map(|(file, _)| file).collect();
    view.assessments = record
        .detail
        .assessments
        .iter()
        .filter(|a| aspect.is_none_or(|s| s == a.assessment.aspect))
        .cloned()
        .collect();
    Ok(view)
}
fn ranges(lines: impl Iterator<Item = usize>) -> Vec<LineRange> {
    let mut ranges: Vec<LineRange> = Vec::new();
    for line in lines {
        if let Some(last) = ranges.last_mut()
            && last.end + 1 == line
        {
            last.end = line;
        } else {
            ranges.push(LineRange {
                start: line,
                end: line,
            });
        }
    }
    ranges
}

/// Exact changed hunks are candidates; assessment reuse remains conservative at file/aspect scope.
fn changed_hunks(path: &str, old: &str, new: &str) -> Result<Vec<CoverageRegion>> {
    let mut options = git2::DiffOptions::new();
    options.context_lines(0);
    let patch = git2::Patch::from_buffers(
        old.as_bytes(),
        Some(Path::new(path)),
        new.as_bytes(),
        Some(Path::new(path)),
        Some(&mut options),
    )?;
    let mut regions = Vec::new();
    for i in 0..patch.num_hunks() {
        let (hunk, _) = patch.hunk(i)?;
        let deleted = hunk.new_lines() == 0;
        let (start, count) = if deleted {
            (hunk.old_start(), hunk.old_lines())
        } else {
            (hunk.new_start(), hunk.new_lines())
        };
        if count > 0 {
            regions.push(CoverageRegion {
                path: path.into(),
                ranges: vec![LineRange {
                    start: start as usize,
                    end: (start + count - 1) as usize,
                }],
                aspect: None,
                reason: if deleted {
                    "deleted hunk (historical line numbers); reconsider its explanations"
                } else {
                    "added or modified hunk"
                }
                .into(),
                document_ids: vec![],
            });
        }
    }
    Ok(regions)
}
