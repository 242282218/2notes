use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Extract a compact 14-digit timestamp (YYYYMMDDHHMMSS) from an RFC3339 string.
/// Falls back to a safe default if parsing fails.
pub fn compact_timestamp(iso: &str) -> String {
    iso.chars()
        .filter(|ch| ch.is_ascii_digit())
        .take(14)
        .collect()
}

/// Format a compact 14-digit stamp into a human filename segment.
/// `with_dashes` controls whether to emit "2026-07-06-114548" (true) or "20260706-114548" (false).
pub fn format_timestamp_for_filename(compact: &str, with_dashes: bool) -> String {
    if compact.len() >= 14 {
        if with_dashes {
            format!(
                "{}-{}-{}-{}{}{}",
                &compact[0..4],
                &compact[4..6],
                &compact[6..8],
                &compact[8..10],
                &compact[10..12],
                &compact[12..14]
            )
        } else {
            format!(
                "{}{}{}-{}{}{}",
                &compact[0..4],
                &compact[4..6],
                &compact[6..8],
                &compact[8..10],
                &compact[10..12],
                &compact[12..14]
            )
        }
    } else if with_dashes {
        "1970-01-01-0000".to_string()
    } else {
        "19700101-000000".to_string()
    }
}

/// Convenience: turn an ISO timestamp into the dashed filename segment used by markdown exports.
pub fn markdown_timestamp(iso: &str) -> String {
    format_timestamp_for_filename(&compact_timestamp(iso), true)
}

/// Convenience: turn an ISO timestamp into the compact segment used by backup filenames.
pub fn backup_timestamp(iso: &str) -> String {
    format_timestamp_for_filename(&compact_timestamp(iso), false)
}

/// Current time as RFC3339 string (best effort).
pub fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_and_format_roundtrip() {
        let iso = "2026-07-06T11:45:48.123Z";
        let compact = compact_timestamp(iso);
        assert_eq!(compact, "20260706114548");
        assert_eq!(markdown_timestamp(iso), "2026-07-06-114548");
        assert_eq!(backup_timestamp(iso), "20260706-114548");
    }

    #[test]
    fn fallback_on_bad_input() {
        assert_eq!(markdown_timestamp("garbage"), "1970-01-01-0000");
        assert_eq!(backup_timestamp(""), "19700101-000000");
    }
}
