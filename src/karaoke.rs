use std::time::Duration;

#[derive(Clone, Debug)]
pub struct LrcLine {
    pub timestamp: Duration,
    pub text: String,
}

pub fn parse_lrc(lrc: &str) -> Vec<LrcLine> {
    let mut lines = Vec::new();
    for line in lrc.lines() {
        let Some(rest) = line.strip_prefix('[') else { continue };
        let Some(bracket_end) = rest.find(']') else { continue };
        let ts_str = &rest[..bracket_end];
        let text = rest[bracket_end + 1..].trim().to_string();
        // Format: mm:ss.xx
        let parts: Vec<&str> = ts_str.splitn(2, ':').collect();
        if parts.len() != 2 { continue; }
        let Ok(mins) = parts[0].parse::<u64>() else { continue };
        let Ok(secs_f) = parts[1].parse::<f64>() else { continue };
        let total_ms = mins * 60_000 + (secs_f * 1000.0) as u64;
        lines.push(LrcLine {
            timestamp: Duration::from_millis(total_ms),
            text,
        });
    }
    lines.sort_by_key(|l| l.timestamp);
    lines
}

pub fn current_line_index(lines: &[LrcLine], pos: Duration) -> usize {
    lines.partition_point(|l| l.timestamp <= pos).saturating_sub(1)
}
