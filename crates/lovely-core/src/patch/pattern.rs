use crop::Rope;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use wildmatch::WildMatch;

use super::InsertPosition;

#[derive(Serialize, Deserialize, Debug)]
pub struct PatternPatch {
    // The pattern that the line will be matched against. Very simple,
    // supports only `?` (one occurrence of any character) and `*` (any number of any character).
    // Patterns are matched against a left-trimmed version of the line, so whitespace does not
    // need to be considered.
    pub pattern: String,

    // The position to insert the target at. `PatternAt::At` replaces the matched line entirely.
    pub position: InsertPosition,
    pub target: String,
    // pub payload_files: Option<Vec<String>>,
    pub payload: String,
    pub match_indent: bool,
    // Apply patch at most `times` times, warn if the number of matches differs from `times`.
    pub times: Option<usize>,

    /// We keep this field around for legacy compat. It doesn't do anything (and never has).
    #[serde(default)]
    pub overwrite: bool,
}

impl PatternPatch {
    /// Apply the pattern patch onto the rope.
    /// The return value will be `true` if the rope was modified.
    pub fn apply(&self, target: &str, rope: &mut Rope) -> bool {
        if self.target != target {
            return false;
        }

        let wm_lines = self
            .pattern
            .lines()
            .map(|x| x.trim())
            .map(WildMatch::new)
            .collect_vec();
        if wm_lines.is_empty() {
            log::warn!("Pattern on target '{target}' has no lines");
            return false;
        }
        let wm_lines_len = wm_lines.len();

        let mut line_index = 0usize;
        let rope_lines = rope.raw_lines().map(|x| x.to_string()).collect_vec();
        let mut matches = Vec::new();
        while let Option::Some(rope_window) = rope_lines.get(line_index..line_index + wm_lines_len)
        {
            if rope_window
                .iter()
                .zip(wm_lines.iter())
                .all(|(source, target)| target.matches(source.trim()))
            {
                if self.match_indent {
                    let leading_indent = String::from_utf8(
                        rope_window[0]
                            .bytes()
                            .take_while(|x| *x == b' ' || *x == b'\t')
                            .collect_vec(),
                    )
                    .unwrap();
                    matches.push((line_index, leading_indent));
                } else {
                    matches.push((line_index, String::new()));
                }
                line_index += wm_lines.len();
            } else {
                line_index += 1;
            }
        }

        if matches.is_empty() {
            log::warn!(
                "Pattern '{}' on target '{target}' resulted in no matches",
                self.pattern.escape_debug()
            );
            return false;
        }
        if let Some(times) = self.times {
            if matches.len() < times {
                if wm_lines_len > 1 {
                    for line in format!(
                        "Pattern '''\n{}''' on target '{target}' resulted in {} matches, wanted {}",
                        self.pattern,
                        matches.len(),
                        times
                    )
                    .lines()
                    {
                        log::warn!("{}", line);
                    }
                } else {
                    log::warn!(
                        "Pattern '{}' on target '{target}' resulted in {} matches, wanted {}",
                        self.pattern,
                        matches.len(),
                        times
                    );
                }
            }
            if matches.len() > times {
                if wm_lines_len > 1 {
                    for line in format!(
                        "Pattern '''\n{}''' on target '{target}' resulted in {} matches, wanted {}",
                        self.pattern,
                        matches.len(),
                        times
                    )
                    .lines()
                    {
                        log::warn!("{}", line);
                    }
                } else {
                    log::warn!(
                        "Pattern '{}' on target '{target}' resulted in {} matches, wanted {}",
                        self.pattern,
                        matches.len(),
                        times
                    );
                }
                log::warn!("Ignoring excess matches");
                matches.truncate(times);
            }
        }

        // Track the +/- index offset caused by previous line injections.
        let mut line_delta = 0;

        for (line_idx, indent) in matches {
            let start = rope.byte_of_line(line_idx + line_delta);
            let end = rope.byte_of_line(line_idx + line_delta + wm_lines_len);

            let mut payload = self
                .payload
                .split_inclusive('\n')
                .format_with("", |x, f| f(&format_args!("{}{}", indent, x)))
                .to_string();
            if !self.payload.ends_with('\n') {
                payload.push('\n');
            }
            let payload_lines = payload.lines().count();

            match self.position {
                InsertPosition::Before => {
                    line_delta += payload_lines;
                    rope.insert(start, &payload);
                }
                InsertPosition::After => {
                    line_delta += payload_lines;
                    rope.insert(end, &payload);
                }
                InsertPosition::At => {
                    line_delta += payload_lines;
                    line_delta -= wm_lines_len;
                    rope.delete(start..end);
                    rope.insert(start, &payload);
                }
            };
        }

        true
    }
}
