use chrono::{DateTime, Utc};

// Convert date to format `2024-02-27  142 days ago`
pub fn date_to_relative_string(date: DateTime<Utc>) -> String {
    let date_str = date
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string();

    let days_diff = (Utc::now() - date).num_days();

    let days_diff_str = if days_diff > 0 {
        format!("{} days ago", days_diff.to_string())
    } else if days_diff == 0 {
        format!("today")
    } else {
        format!("in {} days", (-days_diff).to_string())
    };

    return date_str + "  " + &days_diff_str.to_string();
}
