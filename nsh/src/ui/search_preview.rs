//! One-line search excerpts with literal, case-insensitive keyword emphasis.
use std::ops::Range;

use nsh_core::prelude::*;
use regex::{Regex, RegexBuilder};
use unicode_segmentation::UnicodeSegmentation as _;

use super::{TextRole, Typography as _, accent, fg_secondary};

fn matcher(query: &str) -> Option<Regex> {
    let terms = query
        .split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return None;
    }
    RegexBuilder::new(&terms.join("|"))
        .case_insensitive(true)
        .build()
        .ok()
}

fn ranges(source: &str, matcher: Option<&Regex>) -> Vec<Range<usize>> {
    matcher
        .into_iter()
        .flat_map(|matcher| matcher.find_iter(source).map(|found| found.range()))
        .collect()
}

pub(crate) fn search_preview(
    source: impl Fn() -> String + Copy + 'static,
    query: impl Fn() -> String + Copy + 'static,
) -> Text {
    let pattern = signal_with(None, SignalEquals::Never);
    effect(move || pattern.set(matcher(&query())));
    text("")
        .text_role(TextRole::Body)
        .text_color(fg_secondary)
        .number_of_lines(1)
        .child(dynamic(move || {
            // Bound shaping work like Flutter without splitting emoji sequences.
            let source = source().graphemes(true).take(200).collect::<String>();
            let matches = pattern.with(|matcher| ranges(&source, matcher.as_ref()));
            let mut spans = Vec::new();
            let mut end = 0;
            for matched in matches {
                if matched.start > end {
                    spans.push(text(source[end..matched.start].to_owned()).into_element());
                }
                spans.push(
                    text(source[matched.clone()].to_owned())
                        .text_color(accent)
                        .into_element(),
                );
                end = matched.end;
            }
            if end < source.len() {
                spans.push(text(source[end..].to_owned()).into_element());
            }
            spans
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_case_unicode_and_literal_query_terms_highlight_without_changing_the_excerpt() {
        let source = "Ava: 测试 DESIGN [draft] 👩‍💻 design";
        let matcher = matcher(" 测试 design [draft] ").unwrap();
        let highlights = ranges(source, Some(&matcher))
            .into_iter()
            .map(|range| &source[range])
            .collect::<Vec<_>>();
        assert_eq!(highlights, ["测试", "DESIGN", "[draft]", "design"]);
    }

    #[test]
    fn empty_query_or_unmatched_terms_leave_the_excerpt_unhighlighted() {
        assert!(matcher(" \t ").is_none());
        assert!(ranges("设计 👩‍💻", matcher("missing").as_ref()).is_empty());
    }
}
