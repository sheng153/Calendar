use crate::{
    compile,
    parser::{alarm, helper, monthly, yearly},
};

use std::{fs, path};

pub struct RenderedUnit {
    pub name: String,
    pub contents: String,
}

pub struct RenderedAlarmUnits {
    pub service: RenderedUnit,
    pub timers: Vec<RenderedUnit>,
}

pub fn render_alarms(
    alarms: &[compile::AlarmDefinition],
    bin_path: &str,
) -> Vec<RenderedAlarmUnits> {
    let slugged: Vec<(&compile::AlarmDefinition, String)> = alarms
        .iter()
        .map(|alarm| (alarm, normalized_slug(&alarm.title)))
        .collect();

    let mut totals = std::collections::HashMap::<&str, usize>::new();

    for (_, slug) in &slugged {
        *totals.entry(slug).or_insert(0) += 1;
    }

    let mut used = std::collections::HashMap::<&str, usize>::new();
    let mut rendered = Vec::new();

    for &(alarm, ref slug) in &slugged {
        let suffix = if totals[slug.as_str()] == 1 {
            None
        } else {
            let n = used.entry(slug.as_str()).or_insert(0);
            *n += 1;
            Some(format!("dup{}", *n))
        };

        rendered.push(render_alarm_units(
            alarm,
            bin_path,
            slug.as_str(),
            suffix.as_deref(),
        ));
    }

    rendered
}

fn classify_units_in_dir(
    rendered: &[RenderedAlarmUnits],
    entries: impl IntoIterator<Item = impl AsRef<str>>,
) -> (Vec<String>, Vec<String>) {
    let desired = desired_unit_names(rendered);

    let mut stale_timers = Vec::new();
    let mut stale_services = Vec::new();

    for entry in entries {
        let name = entry.as_ref();

        if !is_managed_alarm_unit(name) || desired.contains(name) {
            continue;
        }

        if name.ends_with(".timer") {
            stale_timers.push(name.to_string());
        } else if name.ends_with(".service") {
            stale_services.push(name.to_string());
        }
    }

    stale_timers.sort();
    stale_services.sort();

    (stale_timers, stale_services)
}

pub fn sync_units(
    rendered: &[RenderedAlarmUnits],
    target_dir: &path::Path,
) -> std::io::Result<Vec<path::PathBuf>> {
    fs::create_dir_all(target_dir)?;

    prune_stale_units(rendered, target_dir, disable_units)?;
    let written = install_units(rendered, target_dir)?;

    Ok(written)
}

fn prune_stale_units(
    rendered: &[RenderedAlarmUnits],
    target_dir: &path::Path,
    disable: impl Fn(&[String]) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let entries = fs::read_dir(target_dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();

    let (stale_timers, stale_services) = classify_units_in_dir(rendered, entries);

    disable(&stale_timers)?;

    for timer in &stale_timers {
        let timer_path = target_dir.join(timer);
        if timer_path.exists() {
            fs::remove_file(timer_path)?;
        }
    }

    for service in &stale_services {
        let service_path = target_dir.join(service);
        if service_path.exists() {
            fs::remove_file(service_path)?;
        }
    }

    Ok(())
}

fn install_units(
    rendered: &[RenderedAlarmUnits],
    target_dir: &path::Path,
) -> std::io::Result<Vec<path::PathBuf>> {
    fs::create_dir_all(target_dir)?;

    let mut written = Vec::new();

    for alarm_units in rendered {
        let service_path = target_dir.join(&alarm_units.service.name);
        fs::write(&service_path, &alarm_units.service.contents)?;
        written.push(service_path);

        for timer in &alarm_units.timers {
            let timer_path = target_dir.join(&timer.name);
            fs::write(&timer_path, &timer.contents)?;
            written.push(timer_path);
        }
    }

    Ok(written)
}

fn desired_unit_names(rendered: &[RenderedAlarmUnits]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();

    for alarm_units in rendered {
        names.insert(alarm_units.service.name.clone());
        for timer in &alarm_units.timers {
            names.insert(timer.name.clone());
        }
    }

    names
}

fn is_managed_alarm_unit(name: &str) -> bool {
    name.starts_with("alarm-") && (name.ends_with(".timer") || name.ends_with(".service"))
}

fn disable_units(unit_names: &[String]) -> std::io::Result<()> {
    for unit_name in unit_names {
        let status = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", unit_name])
            .status()?;

        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to disable {unit_name}"),
            ));
        }
    }

    Ok(())
}

pub fn daemon_reload_user() -> std::io::Result<()> {
    let status = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "systemctl --user daemon-reload failed",
        ))
    }
}

pub fn enable_timers(rendered: &[RenderedAlarmUnits]) -> std::io::Result<()> {
    for alarm_units in rendered {
        for timer in &alarm_units.timers {
            let status = std::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", &timer.name])
                .status()?;

            if !status.success() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("failed to enable {}", timer.name),
                ));
            }
        }
    }

    Ok(())
}

fn render_alarm_units(
    alarm: &compile::AlarmDefinition,
    bin_path: &str,
    slug: &str,
    suffix: Option<&str>,
) -> RenderedAlarmUnits {
    let unit_base = match suffix {
        Some(suffix) => format!("alarm-{slug}-{suffix}"),
        None => format!("alarm-{slug}"),
    };

    let service = RenderedUnit {
        name: format!("{unit_base}.service"),
        contents: render_service(&alarm.title, &alarm.kind, bin_path),
    };

    let timers = alarm
        .schedules
        .iter()
        .enumerate()
        .map(|(idx, schedule)| {
            let timer_base = format!("{unit_base}-{}", idx + 1);
            RenderedUnit {
                name: format!("{timer_base}.timer"),
                contents: render_timer(&alarm.title, &unit_base, schedule),
            }
        })
        .collect();

    RenderedAlarmUnits { service, timers }
}

fn render_service(title: &str, kind: &alarm::AlarmType, bin_path: &str) -> String {
    let escaped_title = shell_escape(title);
    let escaped_kind = shell_escape(&kind.kind_text());

    format!(
        "\
[Unit]
Description=Alarm: {title}

[Service]
Type=oneshot
ExecStart={bin_path} fire --title {escaped_title} --kind {escaped_kind}
"
    )
}

fn render_timer(title: &str, unit_base: &str, schedule: &compile::ScheduleDefinition) -> String {
    let on_calendar = render_on_calendar(schedule);

    format!(
        "\
[Unit]
Description=Timer for alarm: {title}

[Timer]
Unit={unit_base}.service
OnCalendar={on_calendar}
Persistent=true

[Install]
WantedBy=timers.target
"
    )
}

fn render_on_calendar(schedule: &compile::ScheduleDefinition) -> String {
    match schedule {
        compile::ScheduleDefinition::Once { at } => {
            let d = at.date();
            let t = at.time();
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:00",
                d.year, d.month, d.day, t.hour, t.minute
            )
        }
        compile::ScheduleDefinition::Daily { time } => {
            format!("*-*-* {:02}:{:02}:00", time.hour, time.minute)
        }
        compile::ScheduleDefinition::Weekly { days, time } => {
            let weekdays = render_days(*days);

            if weekdays.is_empty() {
                return format!("*-*-* {:02}:{:02}:00", time.hour, time.minute);
            }

            format!("{weekdays} *-*-* {:02}:{:02}:00", time.hour, time.minute)
        }
        compile::ScheduleDefinition::Monthly { rule, time } => match rule {
            monthly::MonthlyRule::DayOfMonth(day) => {
                format!("*-*-{:02} {:02}:{:02}:00", day, time.hour, time.minute)
            }
            monthly::MonthlyRule::NthWeekday(weekday, ordinal) => {
                let weekday = render_weekday(*weekday);
                let ordinal = render_ordinal(*ordinal);
                format!(
                    "{weekday} *-*~{ordinal}..* {:02}:{:02}:00",
                    time.hour, time.minute
                )
            }
        },
        compile::ScheduleDefinition::Yearly { rule, time } => match rule {
            yearly::YearlyRule::DayMonth(day, month) => {
                format!(
                    "*-{:02}-{:02} {:02}:{:02}:00",
                    month, day, time.hour, time.minute
                )
            }
            yearly::YearlyRule::NthWeekday(weekday, ordinal) => {
                let weekday = render_weekday(*weekday);
                let ordinal = render_ordinal(*ordinal);
                format!(
                    "{weekday} *-*-*~{ordinal} {:02}:{:02}:00",
                    time.hour, time.minute
                )
            }
        },
    }
}

fn render_days(days: helper::Days) -> String {
    [
        (helper::Weekday::Lunes, "Mon"),
        (helper::Weekday::Martes, "Tue"),
        (helper::Weekday::Miercoles, "Wed"),
        (helper::Weekday::Jueves, "Thu"),
        (helper::Weekday::Viernes, "Fri"),
        (helper::Weekday::Sabado, "Sat"),
        (helper::Weekday::Domingo, "Sun"),
    ]
    .into_iter()
    .filter(|(day, _)| days.contains(*day))
    .map(|(_, name)| name)
    .collect::<Vec<_>>()
    .join(",")
}

fn render_weekday(weekday: helper::Weekday) -> &'static str {
    match weekday {
        helper::Weekday::Lunes => "Mon",
        helper::Weekday::Martes => "Tue",
        helper::Weekday::Miercoles => "Wed",
        helper::Weekday::Jueves => "Thu",
        helper::Weekday::Viernes => "Fri",
        helper::Weekday::Sabado => "Sat",
        helper::Weekday::Domingo => "Sun",
    }
}

fn render_ordinal(ordinal: u8) -> &'static str {
    match ordinal {
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        other => {
            debug_assert!(false, "render_ordinal: unexpected ordinal {other}");
            "1"
        }
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::new();

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }

    out.trim_matches('-').to_string()
}

fn normalized_slug(title: &str) -> String {
    let slug = slugify(title);
    if slug.is_empty() {
        "unnamed".to_string()
    } else {
        slug
    }
}

fn shell_escape(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::{
        RenderedAlarmUnits, RenderedUnit, classify_units_in_dir, is_managed_alarm_unit,
        normalized_slug, render_alarms, shell_escape,
    };
    use crate::compile::{AlarmDefinition, ScheduleDefinition};
    use crate::parser::alarm::AlarmType;
    use crate::parser::time::{DateSpec, TimeSpec};

    fn alarm(title: &str, kind: AlarmType, schedules: Vec<ScheduleDefinition>) -> AlarmDefinition {
        AlarmDefinition {
            title: title.to_string(),
            kind,
            schedules,
        }
    }

    #[test]
    fn normalized_slug_slugifies_title() {
        assert_eq!(normalized_slug("Wake Up!"), "wake-up");
        assert_eq!(normalized_slug("mañana"), "maana");
        assert_eq!(normalized_slug("___"), "unnamed");
    }

    #[test]
    fn shell_escape_wraps_and_escapes_single_quotes() {
        assert_eq!(shell_escape("plain"), "'plain'");
        assert_eq!(shell_escape("Bob's alarm"), "'Bob'\\''s alarm'");
    }

    #[test]
    fn managed_alarm_unit_recognizes_only_expected_names() {
        assert!(is_managed_alarm_unit("alarm-wake-up.timer"));
        assert!(is_managed_alarm_unit("alarm-wake-up.service"));
        assert!(!is_managed_alarm_unit("wake-up.timer"));
        assert!(!is_managed_alarm_unit("alarm-wake-up.socket"));
    }

    #[test]
    fn classify_units_in_dir_finds_stale_managed_units_only() {
        let rendered = vec![RenderedAlarmUnits {
            service: RenderedUnit {
                name: "alarm-keep.service".into(),
                contents: String::new(),
            },
            timers: vec![RenderedUnit {
                name: "alarm-keep-1.timer".into(),
                contents: String::new(),
            }],
        }];

        let entries = vec![
            "alarm-keep.service",
            "alarm-keep-1.timer",
            "alarm-old.service",
            "alarm-old-1.timer",
            "notes.txt",
            "other.timer",
        ];

        let (stale_timers, stale_services) = classify_units_in_dir(&rendered, entries);

        assert_eq!(stale_timers, vec!["alarm-old-1.timer"]);
        assert_eq!(stale_services, vec!["alarm-old.service"]);
    }

    #[test]
    fn render_alarms_adds_dup_suffix_for_colliding_slugs() {
        let alarms = vec![
            alarm(
                "Wake Up",
                AlarmType::Default,
                vec![ScheduleDefinition::Daily {
                    time: TimeSpec::new(8, 0),
                }],
            ),
            alarm(
                "Wake-Up",
                AlarmType::Important,
                vec![ScheduleDefinition::Daily {
                    time: TimeSpec::new(9, 0),
                }],
            ),
        ];

        let rendered = render_alarms(&alarms, "/bin/alarm");

        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].service.name, "alarm-wake-up-dup1.service");
        assert_eq!(rendered[1].service.name, "alarm-wake-up-dup2.service");
    }

    #[test]
    fn render_alarms_renders_service_and_timer_contents() {
        let alarms = vec![alarm(
            "Wake Up",
            AlarmType::Important,
            vec![ScheduleDefinition::Daily {
                time: TimeSpec::new(8, 30),
            }],
        )];

        let rendered = render_alarms(&alarms, "/usr/local/bin/alarm");

        let unit = &rendered[0];
        assert!(
            unit.service
                .contents
                .contains("ExecStart=/usr/local/bin/alarm fire")
        );
        assert!(unit.service.contents.contains("--title 'Wake Up'"));
        assert!(unit.service.contents.contains("--kind 'important'"));

        assert_eq!(unit.timers.len(), 1);
        assert!(
            unit.timers[0]
                .contents
                .contains("OnCalendar=*-*-* 08:30:00")
        );
        assert!(
            unit.timers[0]
                .contents
                .contains("Unit=alarm-wake-up.service")
        );
    }

    #[test]
    fn render_once_schedule_uses_full_timestamp() {
        let alarms = vec![alarm(
            "Doctor",
            AlarmType::Default,
            vec![ScheduleDefinition::Once {
                at: (TimeSpec::new(14, 5), DateSpec::new(9, 4, 2027)).into(),
            }],
        )];

        let rendered = render_alarms(&alarms, "/bin/alarm");
        assert!(
            rendered[0].timers[0]
                .contents
                .contains("OnCalendar=2027-04-09 14:05:00")
        );
    }
}
