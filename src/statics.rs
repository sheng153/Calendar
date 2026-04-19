use crate::parser::{alarm, daily, monthly, once, parseable, reference, weekly, yearly};

static ALARM_CHAIN: &[&dyn parseable::Parseable] = &[
    &alarm::AlarmParser,
    &once::OnceScheduler,
    &daily::DailyParser,
    &monthly::MonthlyParser,
    &weekly::WeeklyParser,
    &yearly::YearlyParser,
];
static PLAIN_CHAIN: &[&dyn parseable::Parseable] = &[&reference::ReferenceParser];
static COMPOSITE_CHAIN: &[&dyn parseable::Parseable] = &[
    &reference::ReferenceParser,
    &alarm::AlarmParser,
    &once::OnceScheduler,
    &daily::DailyParser,
    &monthly::MonthlyParser,
    &weekly::WeeklyParser,
    &yearly::YearlyParser,
];

pub fn alarm_chain() -> &'static [&'static dyn parseable::Parseable] {
    ALARM_CHAIN
}

pub fn plain_chain() -> &'static [&'static dyn parseable::Parseable] {
    PLAIN_CHAIN
}

pub fn composite_chain() -> &'static [&'static dyn parseable::Parseable] {
    COMPOSITE_CHAIN
}

pub fn main_scheduler_path() -> std::path::PathBuf {
    std::env::var("MAIN_SCHEDULER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("../Events.md"))
}

pub fn systemd_config_user() -> std::path::PathBuf {
    std::env::var("SYSTEMD_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::home_dir()
                .expect("Failed to locate Home dir")
                .join(".config/systemd/user")
        })
}
