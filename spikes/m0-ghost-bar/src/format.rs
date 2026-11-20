// SPDX-License-Identifier: GPL-3.0-or-later

pub fn size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    loop {
        v /= 1024.0;
        if v < 1024.0 || i == units.len() - 1 {
            return format!("{v:.1} {}", units[i]);
        }
        i += 1;
    }
}

/// `YYYY-MM-DD HH:MM` (UTC) from Unix seconds. Howard Hinnant's civil-from-days.
pub fn date(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3_600, rem % 3_600 / 60)
}

pub fn kind(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "Folder".into();
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => format!("{} file", ext.to_uppercase()),
        _ => "File".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(742_391_808), "708.0 MB");
        assert_eq!(date(0), "1970-01-01 00:00");
        assert_eq!(date(1_760_000_000), "2025-10-09 08:53");
        assert_eq!(kind("a.pdf", false), "PDF file");
        assert_eq!(kind(".zshrc", false), "File");
        assert_eq!(kind("x", true), "Folder");
    }
}
