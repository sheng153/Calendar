use crate::errors;
use crate::parser::{alarm, file, helper, monthly, time, yearly};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CompiledFile {
    alarms: Vec<AlarmDefinition>,
    errors: Vec<errors::Error>,
}

impl CompiledFile {
    pub fn new(alarms: Vec<AlarmDefinition>, errors: Vec<errors::Error>) -> Self {
        Self { alarms, errors }
    }

    pub fn alarms(&self) -> &[AlarmDefinition] {
        &self.alarms
    }

    pub fn errors(&self) -> &[errors::Error] {
        &self.errors
    }

    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn compile_alarms(parsed: &[helper::Node]) -> CompiledFile {
    let mut alarms = Vec::new();
    let mut errors = Vec::new();
    let mut pending: Option<AlarmDefinition> = None;

    for node in parsed {
        match node {
            helper::Node::Alarm(alarm) => {
                flush_pending(&mut pending, &mut alarms, &mut errors);

                pending = Some(AlarmDefinition::new(
                    alarm.name().to_string(),
                    alarm.kind().clone(),
                ));
            }

            helper::Node::AlarmSchedule(schedule) => {
                let Some(current) = pending.as_mut() else {
                    errors.push(errors::Error::no_alarm());
                    continue;
                };

                current.add_schedule((*schedule).into());
            }

            helper::Node::Reference(_) => unreachable!(),
        }
    }

    flush_pending(&mut pending, &mut alarms, &mut errors);

    CompiledFile::new(alarms, errors)
}

fn flush_pending(
    pending: &mut Option<AlarmDefinition>,
    alarms: &mut Vec<AlarmDefinition>,
    errors: &mut Vec<errors::Error>,
) {
    let Some(prev) = pending.take() else {
        return;
    };

    if prev.schedules.is_empty() {
        errors.push(errors::Error::alarm_no_schedule(prev.title));
        return;
    }

    alarms.push(prev);
}

pub fn expand_references(
    parsed: impl Into<file::ParsedFile>,
    current_path: &Path,
    stack: &mut Vec<PathBuf>,
) -> (Vec<helper::Node>, Vec<errors::Error>) {
    let parsed: file::ParsedFile = parsed.into();

    let mut alarms = parsed.alarms().to_vec();
    let mut errors = Vec::new();

    for reference in parsed.references() {
        let target_path = current_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(reference.target());

        if stack.contains(&target_path) {
            continue;
        }

        let imported = match file::ParsedFile::from_path(&target_path) {
            Ok(imported) => imported,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };

        if !imported.is_clean() {
            for error in imported.errors() {
                errors.push(errors::Error::parse_in_reference(
                    target_path.display().to_string(),
                    error.clone(),
                ));
            }
            continue;
        }

        stack.push(target_path.clone());
        let (imported_nodes, imported_errors) = expand_references(imported, &target_path, stack);
        stack.pop();

        alarms.extend(imported_nodes);
        errors.extend(imported_errors);
    }

    (alarms, errors)
}

#[derive(Debug, Clone)]
pub struct AlarmDefinition {
    pub title: String,
    pub kind: alarm::AlarmType,
    pub schedules: Vec<ScheduleDefinition>,
}

impl AlarmDefinition {
    fn new(title: String, kind: alarm::AlarmType) -> Self {
        AlarmDefinition {
            title,
            kind,
            schedules: Vec::new(),
        }
    }

    fn add_schedule(&mut self, schedule: ScheduleDefinition) {
        self.schedules.push(schedule);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ScheduleDefinition {
    Once {
        at: time::DateTimeSpec,
    },
    Daily {
        time: time::TimeSpec,
    },
    Weekly {
        days: helper::Days,
        time: time::TimeSpec,
    },
    Monthly {
        rule: monthly::MonthlyRule,
        time: time::TimeSpec,
    },
    Yearly {
        rule: yearly::YearlyRule,
        time: time::TimeSpec,
    },
}

impl From<alarm::AlarmSchedule> for ScheduleDefinition {
    fn from(schedule: alarm::AlarmSchedule) -> Self {
        match schedule.repeat() {
            helper::RepeatSpec::Never => Self::Once {
                at: schedule.at().expect("once schedule should have datetime"),
            },
            helper::RepeatSpec::Daily => Self::Daily {
                time: schedule
                    .at()
                    .expect("daily schedule should have time")
                    .time(),
            },
            helper::RepeatSpec::Weekly(days) => Self::Weekly {
                days,
                time: schedule
                    .at()
                    .expect("weekly schedule should have time")
                    .time(),
            },
            helper::RepeatSpec::Monthly(rule) => Self::Monthly {
                rule,
                time: schedule
                    .at()
                    .expect("monthly schedule should have time")
                    .time(),
            },
            helper::RepeatSpec::Yearly(rule) => Self::Yearly {
                rule,
                time: schedule
                    .at()
                    .expect("yearly schedule should have time")
                    .time(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ScheduleDefinition, compile_alarms};
    use crate::parser::{
        alarm::{Alarm, AlarmSchedule, AlarmType},
        helper::{Days, Node, RepeatSpec, Weekday},
        monthly::MonthlyRule,
        time::{DateSpec, TimeSpec},
        yearly::YearlyRule,
    };

    #[test]
    fn compiles_single_alarm_with_daily_schedule() {
        let nodes = vec![
            Node::Alarm(Alarm::new("Wake up", AlarmType::Default)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(8, 30), DateSpec::zero()).into()),
                RepeatSpec::Daily,
            )),
        ];

        let compiled = compile_alarms(&nodes);

        assert!(compiled.is_clean());
        assert_eq!(compiled.errors().len(), 0);
        assert_eq!(compiled.alarms().len(), 1);

        let alarm = &compiled.alarms()[0];
        assert_eq!(alarm.title, "Wake up");
        assert!(matches!(alarm.kind, AlarmType::Default));
        assert_eq!(alarm.schedules.len(), 1);

        match alarm.schedules[0] {
            ScheduleDefinition::Daily { time } => {
                assert_eq!(time.hour, 8);
                assert_eq!(time.minute, 30);
            }
            _ => panic!("expected daily schedule"),
        }
    }

    #[test]
    fn reports_schedule_without_alarm() {
        let nodes = vec![Node::AlarmSchedule(AlarmSchedule::new(
            Some((TimeSpec::new(9, 0), DateSpec::zero()).into()),
            RepeatSpec::Daily,
        ))];

        let compiled = compile_alarms(&nodes);

        assert!(!compiled.is_clean());
        assert_eq!(compiled.alarms().len(), 0);
        assert_eq!(compiled.errors().len(), 1);
        assert!(format!("{}", compiled.errors()[0]).contains("Schedule Without Alarm"));
    }

    #[test]
    fn reports_alarm_without_schedule() {
        let nodes = vec![Node::Alarm(Alarm::new("Lonely", AlarmType::Important))];

        let compiled = compile_alarms(&nodes);

        assert!(!compiled.is_clean());
        assert_eq!(compiled.alarms().len(), 0);
        assert_eq!(compiled.errors().len(), 1);
        assert!(format!("{}", compiled.errors()[0]).contains("Alarm Without Schedule: Lonely"));
    }

    #[test]
    fn flushes_previous_alarm_when_new_alarm_starts() {
        let nodes = vec![
            Node::Alarm(Alarm::new("First", AlarmType::Default)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(7, 0), DateSpec::zero()).into()),
                RepeatSpec::Daily,
            )),
            Node::Alarm(Alarm::new("Second", AlarmType::Important)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(9, 15), DateSpec::zero()).into()),
                RepeatSpec::Daily,
            )),
        ];

        let compiled = compile_alarms(&nodes);

        assert!(compiled.is_clean());
        assert_eq!(compiled.alarms().len(), 2);
        assert_eq!(compiled.alarms()[0].title, "First");
        assert_eq!(compiled.alarms()[1].title, "Second");
    }

    #[test]
    fn previous_alarm_without_schedule_becomes_error_even_if_next_alarm_is_valid() {
        let nodes = vec![
            Node::Alarm(Alarm::new("Broken", AlarmType::Default)),
            Node::Alarm(Alarm::new("Valid", AlarmType::Important)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(6, 45), DateSpec::zero()).into()),
                RepeatSpec::Daily,
            )),
        ];

        let compiled = compile_alarms(&nodes);

        assert!(!compiled.is_clean());
        assert_eq!(compiled.errors().len(), 1);
        assert!(format!("{}", compiled.errors()[0]).contains("Alarm Without Schedule: Broken"));

        assert_eq!(compiled.alarms().len(), 1);
        assert_eq!(compiled.alarms()[0].title, "Valid");
    }

    #[test]
    fn attaches_multiple_schedules_to_same_alarm() {
        let nodes = vec![
            Node::Alarm(Alarm::new("Hydrate", AlarmType::Default)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(9, 0), DateSpec::zero()).into()),
                RepeatSpec::Daily,
            )),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(15, 0), DateSpec::zero()).into()),
                RepeatSpec::Daily,
            )),
        ];

        let compiled = compile_alarms(&nodes);

        assert!(compiled.is_clean());
        assert_eq!(compiled.alarms().len(), 1);
        assert_eq!(compiled.alarms()[0].schedules.len(), 2);
    }

    #[test]
    fn converts_once_schedule() {
        let nodes = vec![
            Node::Alarm(Alarm::new("Doctor", AlarmType::Default)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(14, 5), DateSpec::new(9, 4, 2027)).into()),
                RepeatSpec::Never,
            )),
        ];

        let compiled = compile_alarms(&nodes);
        let schedule = compiled.alarms()[0].schedules[0];

        match schedule {
            ScheduleDefinition::Once { at } => {
                assert_eq!(at.date().day, 9);
                assert_eq!(at.date().month, 4);
                assert_eq!(at.date().year, 2027);
                assert_eq!(at.time().hour, 14);
                assert_eq!(at.time().minute, 5);
            }
            _ => panic!("expected once schedule"),
        }
    }

    #[test]
    fn converts_weekly_schedule() {
        let mut days = Days::none();
        days.insert(Weekday::Lunes);
        days.insert(Weekday::Viernes);

        let nodes = vec![
            Node::Alarm(Alarm::new("Gym", AlarmType::Default)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(6, 0), DateSpec::zero()).into()),
                RepeatSpec::Weekly(days),
            )),
        ];

        let compiled = compile_alarms(&nodes);
        let schedule = compiled.alarms()[0].schedules[0];

        match schedule {
            ScheduleDefinition::Weekly { days, time } => {
                assert!(days.contains(Weekday::Lunes));
                assert!(days.contains(Weekday::Viernes));
                assert!(!days.contains(Weekday::Martes));
                assert_eq!(time.hour, 6);
                assert_eq!(time.minute, 0);
            }
            _ => panic!("expected weekly schedule"),
        }
    }

    #[test]
    fn converts_monthly_schedule() {
        let nodes = vec![
            Node::Alarm(Alarm::new("Rent", AlarmType::Default)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(10, 0), DateSpec::zero()).into()),
                RepeatSpec::Monthly(MonthlyRule::DayOfMonth(1)),
            )),
        ];

        let compiled = compile_alarms(&nodes);
        let schedule = compiled.alarms()[0].schedules[0];

        match schedule {
            ScheduleDefinition::Monthly { rule, time } => {
                match rule {
                    MonthlyRule::DayOfMonth(day) => assert_eq!(day, 1),
                    _ => panic!("expected day-of-month rule"),
                }
                assert_eq!(time.hour, 10);
                assert_eq!(time.minute, 0);
            }
            _ => panic!("expected monthly schedule"),
        }
    }

    #[test]
    fn converts_yearly_schedule() {
        let nodes = vec![
            Node::Alarm(Alarm::new("Christmas", AlarmType::Important)),
            Node::AlarmSchedule(AlarmSchedule::new(
                Some((TimeSpec::new(7, 30), DateSpec::zero()).into()),
                RepeatSpec::Yearly(YearlyRule::DayMonth(25, 12)),
            )),
        ];

        let compiled = compile_alarms(&nodes);
        let schedule = compiled.alarms()[0].schedules[0];

        match schedule {
            ScheduleDefinition::Yearly { rule, time } => {
                match rule {
                    YearlyRule::DayMonth(day, month) => {
                        assert_eq!(day, 25);
                        assert_eq!(month, 12);
                    }
                    _ => panic!("expected day/month yearly rule"),
                }
                assert_eq!(time.hour, 7);
                assert_eq!(time.minute, 30);
            }
            _ => panic!("expected yearly schedule"),
        }
    }
}
