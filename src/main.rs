use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;

use chrono::NaiveDate;
use chrono_humanize::{Accuracy, HumanTime, Tense};
use clap::Parser;
use time_tracker::parse_timelog;

/// Summarize how work time was spent across categories within a date range.
#[derive(Parser, Debug)]
#[command(about, long_about = None)]
struct Args {
    /// Path to the time log file.
    file: PathBuf,

    /// Length of a working day, e.g. "7.5h", "450m", or "7h30m".
    /// Each day with at least one entry contributes this much total time.
    #[arg(long, default_value = "7.5h", value_parser = parse_day_length)]
    day_length: f64,

    /// Start date (inclusive), formatted YYYY-MM-DD.
    #[arg(long)]
    from: NaiveDate,

    /// End date (inclusive), formatted YYYY-MM-DD.
    #[arg(long)]
    to: NaiveDate,
}

/// Parse a human duration such as "7.5h", "450m", "30s", or "1h30m" into seconds.
/// A bare number (e.g. "7.5") is interpreted as hours.
fn parse_day_length(s: &str) -> Result<f64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("duration is empty".to_string());
    }

    // A bare number is interpreted as hours.
    if let Ok(hours) = s.parse::<f64>() {
        return Ok(hours * 3600.0);
    }

    let mut total = 0.0;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }

        let value: f64 = num
            .parse()
            .map_err(|_| format!("invalid number before '{c}' in {s:?}"))?;
        let multiplier = match c.to_ascii_lowercase() {
            'h' => 3600.0,
            'm' => 60.0,
            's' => 1.0,
            other => return Err(format!("invalid duration unit '{other}' in {s:?}")),
        };
        total += value * multiplier;
        num.clear();
    }

    if !num.is_empty() {
        return Err(format!("number without a unit in {s:?}"));
    }

    Ok(total)
}

/// Format a duration in seconds as a precise, human-readable string.
fn format_duration(secs: f64) -> String {
    let duration = chrono::Duration::milliseconds((secs * 1000.0).round() as i64);
    HumanTime::from(duration).to_text_en(Accuracy::Precise, Tense::Present)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    if args.to < args.from {
        return Err(format!("--to ({}) is before --from ({})", args.to, args.from).into());
    }

    let content = std::fs::read_to_string(&args.file)?;
    let entries = parse_timelog(&content)?;

    // Keep only entries within the (inclusive) date range, in chronological order.
    let mut in_range: Vec<_> = entries
        .iter()
        .filter(|e| {
            let day = e.date.date_naive();
            day >= args.from && day <= args.to
        })
        .collect();
    in_range.sort_by_key(|e| e.date);

    // Accumulate per-category and per-tag time.
    let mut category_secs: HashMap<String, f64> = HashMap::new();
    let mut tag_secs: HashMap<String, f64> = HashMap::new();
    let mut days: HashSet<NaiveDate> = HashSet::new();

    // Breakdown of time within the Mir category by tag group.
    let mut mir_total_secs = 0.0;
    let mut mir_review_secs = 0.0;
    let mut mir_coding_secs = 0.0;
    let mut mir_spec_secs = 0.0;

    let mut prev_date = None;
    for entry in &in_range {
        let day = entry.date.date_naive();
        days.insert(day);

        // An entry's duration runs from the previous entry on the same day.
        // The first entry of a day has no predecessor and contributes nothing.
        let duration = match prev_date {
            Some(prev) if same_day(prev, entry.date) => (entry.date - prev).num_seconds() as f64,
            _ => 0.0,
        };
        prev_date = Some(entry.date);

        if duration > 0.0 && !entry.non_work {
            if !entry.category.is_empty() {
                *category_secs.entry(entry.category.clone()).or_insert(0.0) += duration;
            }
            // A single entry can carry several tags; each gets the full
            // duration, so tag totals may overlap and exceed 100%.
            for tag in &entry.tags {
                *tag_secs.entry(tag.clone()).or_insert(0.0) += duration;
            }

            // Within the Mir category, bucket time by tag group. "review"
            // takes precedence, so "coding"/"spec" only count when the entry
            // is not also a review.
            if entry.category.eq_ignore_ascii_case("mir") {
                let has = |tag: &str| entry.tags.iter().any(|t| t.eq_ignore_ascii_case(tag));
                let review = has("review");
                mir_total_secs += duration;
                if review {
                    mir_review_secs += duration;
                }
                if has("coding") && !review {
                    mir_coding_secs += duration;
                }
                if has("spec") && !review {
                    mir_spec_secs += duration;
                }
            }
        }
    }

    let expected_total = args.day_length * days.len() as f64;
    let tracked: f64 = category_secs.values().sum();
    let untracked = (expected_total - tracked).max(0.0);

    println!("Time stats from {} to {}:", args.from, args.to);
    println!("Category:");

    if expected_total == 0.0 {
        println!("  (no entries in range)");
        return Ok(());
    }

    let percent = |secs: f64| (secs / expected_total * 100.0).round() as i64;

    // Sort categories by time spent, descending (ties broken by name).
    let mut ranked: Vec<(&String, f64)> = category_secs.iter().map(|(c, s)| (c, *s)).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    for (category, secs) in &ranked {
        println!(
            "  {category}: {}% ({})",
            percent(*secs),
            format_duration(*secs)
        );
    }
    println!(
        "  untracked: {}% ({})",
        percent(untracked),
        format_duration(untracked)
    );

    // Sort tags by time spent, descending (ties broken by name).
    let mut tags_ranked: Vec<(&String, f64)> = tag_secs.iter().map(|(t, s)| (t, *s)).collect();
    tags_ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    println!("Tag:");
    if tags_ranked.is_empty() {
        println!("  (no tags in range)");
    } else {
        // An entry may have multiple tags, so these percentages can sum past 100%.
        for (tag, secs) in &tags_ranked {
            println!("  {tag}: {}% ({})", percent(*secs), format_duration(*secs));
        }
    }

    // Breakdown of time within the Mir category, as a share of Mir time.
    println!("Mir breakdown:");
    if mir_total_secs == 0.0 {
        println!("  (no Mir entries in range)");
    } else {
        let mir_percent = |secs: f64| (secs / mir_total_secs * 100.0).round() as i64;
        println!(
            "  review: {}% ({})",
            mir_percent(mir_review_secs),
            format_duration(mir_review_secs)
        );
        println!(
            "  coding (not review): {}% ({})",
            mir_percent(mir_coding_secs),
            format_duration(mir_coding_secs)
        );
        println!(
            "  spec (not review): {}% ({})",
            mir_percent(mir_spec_secs),
            format_duration(mir_spec_secs)
        );
    }

    Ok(())
}

fn same_day(a: chrono::DateTime<chrono::Utc>, b: chrono::DateTime<chrono::Utc>) -> bool {
    a.date_naive() == b.date_naive()
}
