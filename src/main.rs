use std::error::Error;
use std::fmt;

use chrono::{DateTime, NaiveDateTime, Utc};
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "timelog.pest"]
struct TimelogParser;

/// A single parsed time-log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub date: DateTime<Utc>,
    /// Empty when the line has no explicit category.
    pub category: String,
    pub description: String,
    pub tags: Vec<String>,
    /// True when the line is marked as non-work (a "**" marker).
    pub non_work: bool,
}

/// Error returned when parsing a timelog file fails.
#[derive(Debug)]
pub enum ParseTimelogError {
    Grammar(Box<pest::error::Error<Rule>>),
    DateTime(chrono::ParseError),
}

impl fmt::Display for ParseTimelogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseTimelogError::Grammar(e) => write!(f, "failed to parse timelog: {e}"),
            ParseTimelogError::DateTime(e) => write!(f, "failed to parse date/time: {e}"),
        }
    }
}

impl Error for ParseTimelogError {}

impl From<pest::error::Error<Rule>> for ParseTimelogError {
    fn from(e: pest::error::Error<Rule>) -> Self {
        ParseTimelogError::Grammar(Box::new(e))
    }
}

impl From<chrono::ParseError> for ParseTimelogError {
    fn from(e: chrono::ParseError) -> Self {
        ParseTimelogError::DateTime(e)
    }
}

/// Parse the contents of a timelog file into a vector of [`LogEntry`].
pub fn parse_timelog(input: &str) -> Result<Vec<LogEntry>, ParseTimelogError> {
    let file = TimelogParser::parse(Rule::file, input)?
        .next()
        .expect("the `file` rule always produces exactly one pair");

    let mut entries = Vec::new();

    for record in file.into_inner() {
        if record.as_rule() != Rule::entry {
            continue; // EOI, blank lines, etc.
        }

        let mut date_str = "";
        let mut time_str = "";
        let mut category = String::new();
        let mut description = String::new();
        let mut tags = Vec::new();
        let mut non_work = false;

        for part in record.into_inner() {
            match part.as_rule() {
                Rule::date => date_str = part.as_str(),
                Rule::time => time_str = part.as_str(),
                Rule::category => category = part.as_str().trim().to_string(),
                Rule::description => description = part.as_str().trim().to_string(),
                Rule::nonwork => non_work = true,
                Rule::tag_list => {
                    for tag in part.into_inner() {
                        if tag.as_rule() == Rule::tag {
                            tags.push(tag.as_str().to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        let naive =
            NaiveDateTime::parse_from_str(&format!("{date_str} {time_str}"), "%Y-%m-%d %H:%M")?;

        entries.push(LogEntry {
            date: naive.and_utc(),
            category,
            description,
            tags,
            non_work,
        });
    }

    Ok(entries)
}

fn main() -> Result<(), Box<dyn Error>> {
    let input = std::fs::read_to_string("timelog.txt")?;
    let entries = parse_timelog(&input)?;

    println!("Parsed {} entries.\n", entries.len());
    for entry in &entries {
        println!(
            "{} | category={:<14} | non_work={:<5} | tags={:?}\n    {}",
            entry.date.format("%Y-%m-%d %H:%M"),
            format!("{:?}", entry.category),
            entry.non_work,
            entry.tags,
            entry.description,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> DateTime<Utc> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
            .unwrap()
            .and_utc()
    }

    #[test]
    fn no_category_attached_nonwork() {
        let entries = parse_timelog("2026-04-07 10:31: arrived**\n").unwrap();
        assert_eq!(
            entries,
            vec![LogEntry {
                date: dt("2026-04-07 10:31"),
                category: String::new(),
                description: "arrived".to_string(),
                tags: vec![],
                non_work: true,
            }]
        );
    }

    #[test]
    fn category_and_description() {
        let entries = parse_timelog("2026-04-07 11:10: managering: Set up time logging\n").unwrap();
        let e = &entries[0];
        assert_eq!(e.category, "managering");
        assert_eq!(e.description, "Set up time logging");
        assert!(e.tags.is_empty());
    }

    #[test]
    fn spaced_nonwork_no_category() {
        let e = &parse_timelog("2026-04-07 12:29: tea **\n").unwrap()[0];
        assert_eq!(e.category, "");
        assert_eq!(e.description, "tea");
        assert!(e.non_work);
    }

    #[test]
    fn colon_in_description_is_not_category() {
        let e = &parse_timelog("2026-04-10 18:04: meeting: Tarek 1:1\n").unwrap()[0];
        assert_eq!(e.category, "meeting");
        assert_eq!(e.description, "Tarek 1:1");
    }

    #[test]
    fn category_with_space() {
        let e =
            &parse_timelog("2026-06-15 10:20: Archive Admin: process-removals fiddling -- AA\n")
                .unwrap()[0];
        assert_eq!(e.category, "Archive Admin");
        assert_eq!(e.description, "process-removals fiddling");
        assert_eq!(e.tags, vec!["AA"]);
    }

    #[test]
    fn tag_list_and_extra_spaces() {
        let e = &parse_timelog("2026-05-25 11:57: mir: Reviews --  mir review\n").unwrap()[0];
        assert_eq!(e.category, "mir");
        assert_eq!(e.description, "Reviews");
        assert_eq!(e.tags, vec!["mir", "review"]);
        assert!(!e.non_work);
    }

    #[test]
    fn dash_in_description() {
        let e = &parse_timelog(
            "2026-05-25 14:16: meeting: ad-hoc code review w/ Robert -- mir meeting\n",
        )
        .unwrap()[0];
        assert_eq!(e.description, "ad-hoc code review w/ Robert");
        assert_eq!(e.tags, vec!["mir", "meeting"]);
    }

    #[test]
    fn blank_lines_are_skipped() {
        let input = "2026-05-21 10:26: arrive **\n\n2026-05-22 06:36: arrive **\n";
        assert_eq!(parse_timelog(input).unwrap().len(), 2);
    }
}
