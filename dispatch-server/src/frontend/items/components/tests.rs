use super::*;
use assertr::prelude::*;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[test]
fn formats_claim_elapsed_time() {
    assert_that!(&(format_claim_elapsed_seconds(70))).is_equal_to("1:10");
    assert_that!(&(format_claim_elapsed_seconds(3670))).is_equal_to("1:01:10");
    assert_that!(&(format_claim_elapsed_seconds(-5))).is_equal_to("0:00");
}

#[test]
fn derives_claim_elapsed_time_from_claim_timestamp() {
    let now = OffsetDateTime::parse("2026-06-17T18:01:10Z", &Rfc3339).unwrap();
    assert_that!(&(claim_elapsed_seconds_at("2026-06-17T18:00:00Z", now))).is_equal_to(Some(70));
    assert_that!(&(claim_elapsed_seconds_at("2026-06-17T18:02:00Z", now))).is_equal_to(Some(0));
    assert_that!(&(claim_elapsed_seconds_at("not a timestamp", now))).is_equal_to(None);
}

#[test]
fn preview_omits_rich_text_markup() {
    assert_that!(&(preview("<p>First <strong>item</strong></p><p>Second</p>")))
        .is_equal_to("First item\nSecond");
}
