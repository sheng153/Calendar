use crate::errors;
use crate::statics;

pub fn parse_file(input: &str) -> file::ParsedFile {
    let kind = file::classify_file(input);

    let chain = match kind {
        file::FileType::Alarm => statics::alarm_chain(),
        file::FileType::Composite => statics::composite_chain(),
        file::FileType::Plain => statics::plain_chain(),
    };

    let (nodes, errors) = input.lines().enumerate().fold(
        (Vec::new(), Vec::new()),
        |(mut nodes, mut errors), (idx, line)| {
            match parse_with(chain, line) {
                Ok(Some(node)) => nodes.push(node),
                Ok(None) => {}
                Err(error) => errors.push(errors::LineError::new(idx, line.to_string(), error)),
            }
            (nodes, errors)
        },
    );

    file::ParsedFile::new(nodes, errors)
}

pub fn parse_with(
    parsers: &[&dyn parseable::Parseable],
    input: &str,
) -> Result<Option<helper::Node>, errors::Error> {
    if input.trim().is_empty() {
        return Ok(None);
    }

    for parser in parsers {
        if let Some(node) = parser.handle(input)? {
            return Ok(Some(node));
        }
    }

    Ok(None)
}

pub mod helper {
    use crate::parser::{alarm, errors, time};

    #[derive(PartialEq, Eq, Clone, Copy)]
    pub enum ScheduleType {
        Daily,
        Weekly,
        Monthly,
        Yearly,
        Once,
    }

    pub struct ScheduleLine<'a> {
        parts: Vec<&'a str>,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Weekday {
        Domingo,
        Lunes,
        Martes,
        Miercoles,
        Jueves,
        Viernes,
        Sabado,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Ordinal {
        First,
        Second,
        Third,
        Fourth,
        Last,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Days {
        domingo: bool,
        lunes: bool,
        martes: bool,
        miercoles: bool,
        jueves: bool,
        viernes: bool,
        sabado: bool,
    }

    pub fn parse_date_spec(input: &str) -> Result<time::DateSpec, errors::Error> {
        let parts = input
            .split('/')
            .map(|part| part.parse::<u16>().map_err(|_| errors::Error::Syntax))
            .collect::<Result<Vec<_>, _>>()?;

        match parts.as_slice() {
            [day] => {
                let day = *day as u8;

                if !(1..=31).contains(&day) {
                    return Err(errors::Error::Syntax);
                }

                Ok(time::DateSpec::month_and_year_agnostic(day))
            }
            [day, month] => {
                let (day, month) = (*day as u8, *month as u8);

                if !(1..=31).contains(&day) || !(1..=12).contains(&month) {
                    return Err(errors::Error::Syntax);
                }

                Ok(time::DateSpec::year_agnostic(day, month))
            }
            [day, month, year] => {
                let (day, month, year) = (*day as u8, *month as u8, *year);

                if !(1..=31).contains(&day) || !(1..=12).contains(&month) {
                    return Err(errors::Error::Syntax);
                }

                Ok(time::DateSpec::new(day, month, year))
            }
            _ => Err(errors::Error::Syntax),
        }
    }

    pub fn parse_schedule_line(
        input: &str,
        expected: ScheduleType,
    ) -> Result<Option<ScheduleLine<'_>>, errors::Error> {
        let input = input.trim();

        let Some(rest) = input
            .strip_prefix("schedule:")
            .or_else(|| input.strip_prefix("date:"))
            .map(str::trim)
        else {
            return Ok(None);
        };

        let mut parts = rest.split_whitespace().collect::<Vec<_>>();
        if parts.is_empty() {
            return Err(errors::Error::Syntax);
        }

        let actual = match parts[0].to_ascii_lowercase().as_str() {
            "daily" | "diario" => {
                parts.remove(0);
                ScheduleType::Daily
            }
            "weekly" | "semanal" => {
                parts.remove(0);
                ScheduleType::Weekly
            }
            "monthly" | "mensual" => {
                parts.remove(0);
                ScheduleType::Monthly
            }
            "yearly" | "anual" => {
                parts.remove(0);
                ScheduleType::Yearly
            }
            _ => ScheduleType::Once,
        };

        if actual != expected {
            return Ok(None);
        }

        Ok(Some(ScheduleLine::new(parts)))
    }

    pub fn find_last_unescaped(rest: &str, split_at: char) -> Option<usize> {
        rest.match_indices(split_at)
            .map(|(idx, _)| idx)
            .rev()
            .find(|&idx| {
                rest.as_bytes()[..idx]
                    .iter()
                    .rev()
                    .take_while(|&&b| b == b'\\')
                    .count()
                    % 2
                    == 0
            })
    }

    impl<'a> ScheduleLine<'a> {
        pub const fn new(parts: Vec<&'a str>) -> Self {
            Self { parts }
        }

        pub fn parts(&self) -> &[&'a str] {
            &self.parts
        }
    }

    impl Weekday {
        pub fn new(name: &str) -> Option<Self> {
            let name: &str = &name.to_lowercase();
            match name {
                "domingo" => Some(Self::Domingo),
                "lunes" => Some(Self::Lunes),
                "martes" => Some(Self::Martes),
                "miércoles" | "miercoles" => Some(Self::Miercoles),
                "jueves" => Some(Self::Jueves),
                "viernes" => Some(Self::Viernes),
                "sábado" | "sabado" => Some(Self::Sabado),
                _ => None,
            }
        }
    }

    impl Ordinal {
        pub fn from_str(input: &str) -> Option<Self> {
            let input: &str = &input.to_lowercase();
            match input {
                "primer" => Some(Self::First),
                "segundo" => Some(Self::Second),
                "tercer" => Some(Self::Third),
                "cuarto" => Some(Self::Fourth),
                "último" | "ultimo" => Some(Self::Last),
                _ => None,
            }
        }
    }

    impl From<Ordinal> for u8 {
        fn from(value: Ordinal) -> Self {
            match value {
                Ordinal::First => 1,
                Ordinal::Second => 2,
                Ordinal::Third => 3,
                Ordinal::Fourth | Ordinal::Last => 4,
            }
        }
    }

    impl Days {
        pub const fn none() -> Self {
            Self {
                domingo: false,
                lunes: false,
                martes: false,
                miercoles: false,
                jueves: false,
                viernes: false,
                sabado: false,
            }
        }

        pub const fn insert(&mut self, day: Weekday) {
            match day {
                Weekday::Domingo => self.domingo = true,
                Weekday::Lunes => self.lunes = true,
                Weekday::Martes => self.martes = true,
                Weekday::Miercoles => self.miercoles = true,
                Weekday::Jueves => self.jueves = true,
                Weekday::Viernes => self.viernes = true,
                Weekday::Sabado => self.sabado = true,
            }
        }

        pub const fn contains(self, day: Weekday) -> bool {
            match day {
                Weekday::Domingo => self.domingo,
                Weekday::Lunes => self.lunes,
                Weekday::Martes => self.martes,
                Weekday::Miercoles => self.miercoles,
                Weekday::Jueves => self.jueves,
                Weekday::Viernes => self.viernes,
                Weekday::Sabado => self.sabado,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub enum RepeatSpec {
        Never,
        Daily,
        Weekly(Days),
        Monthly(crate::parser::monthly::MonthlyRule),
        Yearly(crate::parser::yearly::YearlyRule),
    }

    #[derive(Debug, Clone)]
    pub enum Node {
        Reference(crate::parser::reference::Reference),
        Alarm(alarm::Alarm),
        AlarmSchedule(alarm::AlarmSchedule),
    }
}

pub mod parseable {
    use crate::errors;
    use crate::parser::helper;
    pub type Input<'a> = &'a str;
    pub trait Parseable: Sync {
        fn handle(&self, to_be_handled: Input) -> Result<Option<helper::Node>, errors::Error>;
    }
}

pub mod once {
    use crate::errors;
    use crate::parser::{alarm, helper, parseable, time};
    pub struct OnceScheduler;

    impl parseable::Parseable for OnceScheduler {
        fn handle(&self, input: parseable::Input) -> Result<Option<helper::Node>, errors::Error> {
            let Some(line) = helper::parse_schedule_line(input, helper::ScheduleType::Once)? else {
                return Ok(None);
            };

            let [date, time] = line.parts() else {
                return Err(errors::Error::Syntax);
            };

            let date = helper::parse_date_spec(date)?;
            let time = time::parse_time(time)?;

            Ok(Some(helper::Node::AlarmSchedule(
                alarm::AlarmSchedule::new(Some((time, date).into()), helper::RepeatSpec::Never),
            )))
        }
    }
}

pub mod daily {
    use crate::errors;
    use crate::parser::{alarm, helper, parseable, time};
    pub struct DailyParser;

    impl parseable::Parseable for DailyParser {
        fn handle(&self, input: parseable::Input) -> Result<Option<helper::Node>, errors::Error> {
            let Some(line) = helper::parse_schedule_line(input, helper::ScheduleType::Daily)?
            else {
                return Ok(None);
            };

            let [time] = line.parts() else {
                return Err(errors::Error::Syntax);
            };

            let time = time::parse_time(time)?;

            Ok(Some(helper::Node::AlarmSchedule(
                alarm::AlarmSchedule::new(
                    Some((time, time::DateSpec::zero()).into()),
                    helper::RepeatSpec::Daily,
                ),
            )))
        }
    }
}

pub mod weekly {
    use crate::errors;
    use crate::parser::{alarm, helper, parseable, time};
    pub struct WeeklyParser;

    impl parseable::Parseable for WeeklyParser {
        fn handle(&self, input: parseable::Input) -> Result<Option<helper::Node>, errors::Error> {
            let Some(line) = helper::parse_schedule_line(input, helper::ScheduleType::Weekly)?
            else {
                return Ok(None);
            };

            if line.parts().len() < 2 {
                return Err(errors::Error::Syntax);
            }

            let (days, time_part) = line.parts().split_at(line.parts().len() - 1);
            let time = time::parse_time(time_part[0])?;

            let mut weekdays = helper::Days::none();

            for day in days {
                let Some(weekday) = helper::Weekday::new(day) else {
                    return Err(errors::Error::Syntax);
                };

                weekdays.insert(weekday);
            }

            if weekdays == helper::Days::none() {
                return Err(errors::Error::Syntax);
            }

            Ok(Some(helper::Node::AlarmSchedule(
                alarm::AlarmSchedule::new(
                    Some((time, time::DateSpec::zero()).into()),
                    helper::RepeatSpec::Weekly(weekdays),
                ),
            )))
        }
    }
}

pub mod monthly {
    use crate::errors;
    use crate::parser::time as time_mod;
    use crate::parser::{alarm, helper, parseable};
    pub struct MonthlyParser;

    #[derive(Debug, Clone, Copy)]
    pub enum MonthlyRule {
        DayOfMonth(u8),
        NthWeekday(helper::Weekday, u8),
    }

    impl MonthlyRule {
        pub const fn day_of_month(date: u8) -> Self {
            Self::DayOfMonth(date)
        }

        pub fn nth_weekday(weekday: helper::Weekday, ordinal: helper::Ordinal) -> Self {
            Self::NthWeekday(weekday, ordinal.into())
        }
    }

    impl parseable::Parseable for MonthlyParser {
        fn handle(&self, input: parseable::Input) -> Result<Option<helper::Node>, errors::Error> {
            let Some(line) = helper::parse_schedule_line(input, helper::ScheduleType::Monthly)?
            else {
                return Ok(None);
            };

            parse_monthly_line(&line)
        }
    }

    pub fn parse_monthly_line(
        line: &helper::ScheduleLine,
    ) -> Result<Option<helper::Node>, errors::Error> {
        if let Some(node) = parse_monthly_number(line)? {
            return Ok(Some(node));
        }

        if let Some(node) = parse_monthly_weekday(line)? {
            return Ok(Some(node));
        }

        Err(errors::Error::Syntax)
    }

    fn parse_monthly_number(
        line: &helper::ScheduleLine,
    ) -> Result<Option<helper::Node>, errors::Error> {
        let [day, time] = line.parts() else {
            return Ok(None);
        };

        let day = helper::parse_date_spec(day)?;
        let time = time_mod::parse_time(time)?;

        if day.month != 0 || day.year != 0 {
            return Err(errors::Error::Syntax);
        }

        Ok(Some(helper::Node::AlarmSchedule(
            alarm::AlarmSchedule::new(
                Some((time, day).into()),
                helper::RepeatSpec::Monthly(MonthlyRule::day_of_month(day.day)),
            ),
        )))
    }

    fn parse_monthly_weekday(
        line: &helper::ScheduleLine,
    ) -> Result<Option<helper::Node>, errors::Error> {
        let [ordinal, weekday, time] = line.parts() else {
            return Ok(None);
        };

        let Some(ordinal) = helper::Ordinal::from_str(ordinal) else {
            return Err(errors::Error::Syntax);
        };

        let Some(weekday) = helper::Weekday::new(weekday) else {
            return Err(errors::Error::Syntax);
        };

        let time = time_mod::parse_time(time)?;

        Ok(Some(helper::Node::AlarmSchedule(
            alarm::AlarmSchedule::new(
                Some((time, time_mod::DateSpec::zero()).into()),
                helper::RepeatSpec::Monthly(MonthlyRule::nth_weekday(weekday, ordinal.into())),
            ),
        )))
    }
}

pub mod yearly {
    use crate::errors;
    use crate::parser::{alarm, helper, parseable, time};

    #[derive(Debug, Clone, Copy)]
    pub enum YearlyRule {
        DayMonth(u8, u8),
    }

    impl YearlyRule {
        pub const fn new_day_month(date: time::DateSpec) -> Self {
            Self::DayMonth(date.day, date.month)
        }
    }

    pub struct YearlyParser;

    impl parseable::Parseable for YearlyParser {
        fn handle(&self, input: parseable::Input) -> Result<Option<helper::Node>, errors::Error> {
            let Some(line) = helper::parse_schedule_line(input, helper::ScheduleType::Yearly)?
            else {
                return Ok(None);
            };

            let [date, time] = line.parts() else {
                return Err(errors::Error::Syntax);
            };

            let date = helper::parse_date_spec(date)?;
            let time = time::parse_time(time)?;

            if date.year != 0 {
                return Err(errors::Error::Syntax);
            }

            Ok(Some(helper::Node::AlarmSchedule(
                alarm::AlarmSchedule::new(
                    Some((time, date).into()),
                    helper::RepeatSpec::Yearly(YearlyRule::new_day_month(date)),
                ),
            )))
        }
    }
}

pub mod time {
    use crate::errors;
    #[derive(Debug, Clone, Copy)]
    pub struct DateSpec {
        pub year: u16,
        pub month: u8,
        pub day: u8,
    }

    pub fn parse_time(input: &str) -> Result<TimeSpec, errors::Error> {
        let parts = input
            .split(':')
            .map(|part| part.parse::<u8>().map_err(|_| errors::Error::Syntax))
            .collect::<Result<Vec<_>, _>>()?;

        let [hour, minute] = parts.as_slice() else {
            return Err(errors::Error::Syntax);
        };

        if *hour > 23 || *minute > 59 {
            return Err(errors::Error::Syntax);
        }

        Ok(TimeSpec {
            hour: *hour,
            minute: *minute,
        })
    }

    #[derive(Debug, Clone, Copy)]
    pub struct TimeSpec {
        pub hour: u8,
        pub minute: u8,
    }

    impl TimeSpec {
        pub const fn new(hour: u8, minute: u8) -> Self {
            Self { hour, minute }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct DateTimeSpec(DateSpec, TimeSpec);

    impl DateSpec {
        pub const fn zero() -> Self {
            Self {
                year: 0,
                month: 0,
                day: 0,
            }
        }

        pub const fn new(day: u8, month: u8, year: u16) -> Self {
            Self { year, month, day }
        }

        pub const fn month_and_year_agnostic(day: u8) -> Self {
            Self {
                day,
                month: 0u8,
                year: 0u16,
            }
        }

        pub const fn year_agnostic(day: u8, month: u8) -> Self {
            Self {
                day,
                month,
                year: 0u16,
            }
        }
    }

    impl DateTimeSpec {
        pub const fn time(self) -> TimeSpec {
            self.1
        }

        pub const fn date(self) -> DateSpec {
            self.0
        }
    }

    impl From<(DateSpec, TimeSpec)> for DateTimeSpec {
        fn from(value: (DateSpec, TimeSpec)) -> Self {
            Self(value.0, value.1)
        }
    }
    impl From<(TimeSpec, DateSpec)> for DateTimeSpec {
        fn from(value: (TimeSpec, DateSpec)) -> Self {
            Self(value.1, value.0)
        }
    }
}

pub mod alarm {
    use std::borrow::Cow;

    use crate::errors;
    use crate::parser::{helper, parseable, time};

    #[derive(Debug, Clone)]
    pub enum AlarmType {
        Default,
        Insistent,
        Important,
        Personalized(String),
    }

    impl AlarmType {
        pub fn kind_text(&self) -> Cow<'_, str> {
            match self {
                Self::Default => Cow::Borrowed("default"),
                Self::Insistent => Cow::Borrowed("insistent"),
                Self::Important => Cow::Borrowed("important"),
                Self::Personalized(title) => Cow::Owned(title.to_lowercase()),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct Alarm {
        name: String,
        kind: AlarmType,
    }

    impl Alarm {
        pub fn new(name: &str, kind: AlarmType) -> Self {
            Self {
                name: name.to_string(),
                kind,
            }
        }

        pub fn name(&self) -> &str {
            &self.name
        }

        pub const fn kind(&self) -> &AlarmType {
            &self.kind
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct AlarmSchedule {
        at: Option<time::DateTimeSpec>,
        repeat: helper::RepeatSpec,
    }

    impl AlarmSchedule {
        pub const fn new(at: Option<time::DateTimeSpec>, repeat: helper::RepeatSpec) -> Self {
            Self { at, repeat }
        }

        pub const fn at(&self) -> Option<time::DateTimeSpec> {
            self.at
        }

        pub const fn repeat(&self) -> helper::RepeatSpec {
            self.repeat
        }
    }

    pub struct AlarmParser;

    impl parseable::Parseable for AlarmParser {
        fn handle(&self, input: parseable::Input) -> Result<Option<helper::Node>, errors::Error> {
            let input = input.trim();

            let Some(rest) = input
                .strip_prefix("alarm:")
                .or_else(|| input.strip_prefix("title:"))
                .map(str::trim)
            else {
                return Ok(None);
            };

            if rest.is_empty() {
                return Err(errors::Error::Syntax);
            }

            let (name, kind) = if let Some(open) = helper::find_last_unescaped(rest, '(') {
                let name = rest[..open].trim();

                let raw_type = rest[open..]
                    .trim()
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                    .ok_or(errors::Error::Syntax)?
                    .trim();

                if name.is_empty() || raw_type.is_empty() {
                    return Err(errors::Error::Syntax);
                }

                let alarm_type = match raw_type.to_lowercase().as_str() {
                    "insistent" => AlarmType::Insistent,
                    "important" => AlarmType::Important,
                    other => AlarmType::Personalized(other.to_string()),
                };

                (name.replace(r"\(", "(").replace(r"\)", ")"), alarm_type)
            } else {
                (
                    rest.replace(r"\(", "(").replace(r"\)", ")"),
                    AlarmType::Default,
                )
            };

            Ok(Some(helper::Node::Alarm(Alarm::new(&name, kind))))
        }
    }
}

pub mod reference {
    use crate::errors;
    use crate::parser::{helper, parseable};

    #[derive(Debug, Clone)]
    pub struct Reference {
        target: String,
    }

    impl Reference {
        pub const fn new(target: String) -> Self {
            Self { target }
        }

        pub fn target(&self) -> &str {
            &self.target
        }

        pub fn resolve_target(raw: &str) -> String {
            format!("{raw}.md")
        }
    }

    pub struct ReferenceParser;

    impl parseable::Parseable for ReferenceParser {
        fn handle(&self, input: parseable::Input) -> Result<Option<helper::Node>, errors::Error> {
            let input = input.trim();
            let prefix = "- [ ] [[";
            let suffix = "]]";

            if input.starts_with(prefix) && input.ends_with(suffix) {
                let target =
                    Reference::resolve_target(&input[prefix.len()..input.len() - suffix.len()]);
                Ok(Some(helper::Node::Reference(Reference::new(target))))
            } else {
                Ok(None)
            }
        }
    }
}

pub mod file {
    use crate::parser::{errors, helper, parseable, reference};

    #[derive(Debug, Clone, Copy)]
    pub enum FileType {
        Alarm,
        Composite,
        Plain,
    }

    #[derive(Debug)]
    pub struct ParsedFile {
        alarms: Vec<helper::Node>,
        references: Vec<reference::Reference>,
        errors: Vec<errors::LineError>,
    }

    impl std::convert::From<&str> for ParsedFile {
        fn from(value: &str) -> Self {
            crate::parser::parse_file(value)
        }
    }

    impl ParsedFile {
        pub fn new(nodes: Vec<helper::Node>, errors: Vec<errors::LineError>) -> Self {
            let (references, alarms) = nodes.into_iter().fold(
                (
                    Vec::<reference::Reference>::new(),
                    Vec::<helper::Node>::new(),
                ),
                |(mut refs, mut alarms), node| {
                    match node {
                        helper::Node::Reference(inner) => refs.push(inner),
                        other => alarms.push(other),
                    }
                    (refs, alarms)
                },
            );
            Self {
                alarms,
                references,
                errors,
            }
        }

        pub fn from_path(path: &std::path::Path) -> Result<Self, errors::Error> {
            let contents = std::fs::read_to_string(path)
                .map_err(|_| errors::Error::reference_not_found(path.display().to_string()))?;
            Ok(contents.as_str().into())
        }

        pub fn alarms(&self) -> &[helper::Node] {
            &self.alarms
        }

        pub fn references(&self) -> &[reference::Reference] {
            &self.references
        }

        pub fn errors(&self) -> &[errors::LineError] {
            &self.errors
        }

        pub const fn is_clean(&self) -> bool {
            self.errors.is_empty()
        }
    }

    pub fn classify_file(input: parseable::Input) -> FileType {
        let first = input.lines().next().unwrap_or("").trim();

        if first.to_lowercase().contains("alarm") {
            FileType::Alarm
        } else if first.to_lowercase().contains("schedule") {
            FileType::Composite
        } else {
            FileType::Plain
        }
    }
}

#[cfg(test)]
mod tests {
    use super::alarm::{AlarmParser, AlarmType};
    use super::daily::DailyParser;
    use super::helper::{self, Days, Node, Ordinal, RepeatSpec, ScheduleType, Weekday};
    use super::monthly::{MonthlyParser, MonthlyRule};
    use super::once::OnceScheduler;
    use super::parseable::Parseable;
    use super::reference::ReferenceParser;
    use super::time;
    use super::weekly::WeeklyParser;
    use super::yearly::{YearlyParser, YearlyRule};
    use crate::errors::Error;

    #[test]
    fn parse_date_spec_day_only() {
        let d = helper::parse_date_spec("15").unwrap();
        assert_eq!(d.day, 15);
        assert_eq!(d.month, 0);
        assert_eq!(d.year, 0);
    }

    #[test]
    fn parse_date_spec_day_month() {
        let d = helper::parse_date_spec("15/7").unwrap();
        assert_eq!(d.day, 15);
        assert_eq!(d.month, 7);
        assert_eq!(d.year, 0);
    }

    #[test]
    fn parse_date_spec_full_date() {
        let d = helper::parse_date_spec("15/7/2027").unwrap();
        assert_eq!(d.day, 15);
        assert_eq!(d.month, 7);
        assert_eq!(d.year, 2027);
    }

    #[test]
    fn parse_date_spec_rejects_invalid_day() {
        assert!(matches!(helper::parse_date_spec("0"), Err(Error::Syntax)));
        assert!(matches!(helper::parse_date_spec("32"), Err(Error::Syntax)));
    }

    #[test]
    fn parse_schedule_line_daily_matches_expected_type() {
        let line = helper::parse_schedule_line("schedule: daily 08:30", ScheduleType::Daily)
            .unwrap()
            .unwrap();

        assert_eq!(line.parts(), &["08:30"]);
    }

    #[test]
    fn parse_schedule_line_returns_none_for_other_schedule_types() {
        let line = helper::parse_schedule_line("schedule: weekly lunes 08:30", ScheduleType::Daily)
            .unwrap();

        assert!(line.is_none());
    }

    #[test]
    fn find_last_unescaped_finds_real_separator() {
        let s = r"hello \(ignored\) (real)";
        let idx = helper::find_last_unescaped(s, '(').unwrap();
        assert_eq!(&s[idx..], "(real)");
    }

    #[test]
    fn weekday_parses_accented_and_plain_forms() {
        assert!(matches!(
            Weekday::new("miércoles"),
            Some(Weekday::Miercoles)
        ));
        assert!(matches!(
            Weekday::new("miercoles"),
            Some(Weekday::Miercoles)
        ));
        assert!(matches!(Weekday::new("sábado"), Some(Weekday::Sabado)));
        assert!(matches!(Weekday::new("sabado"), Some(Weekday::Sabado)));
    }

    #[test]
    fn ordinal_parses_spanish_words() {
        assert!(matches!(Ordinal::from_str("primer"), Some(Ordinal::First)));
        assert!(matches!(Ordinal::from_str("último"), Some(Ordinal::Last)));
        assert!(matches!(Ordinal::from_str("ultimo"), Some(Ordinal::Last)));
    }

    #[test]
    fn days_insert_and_contains_work() {
        let mut days = Days::none();
        days.insert(Weekday::Lunes);
        days.insert(Weekday::Viernes);

        assert!(days.contains(Weekday::Lunes));
        assert!(days.contains(Weekday::Viernes));
        assert!(!days.contains(Weekday::Martes));
    }

    #[test]
    fn parse_time_accepts_valid_time() {
        let t = time::parse_time("23:59").unwrap();
        assert_eq!(t.hour, 23);
        assert_eq!(t.minute, 59);
    }

    #[test]
    fn parse_time_rejects_invalid_time() {
        assert!(matches!(time::parse_time("24:00"), Err(Error::Syntax)));
        assert!(matches!(time::parse_time("12:60"), Err(Error::Syntax)));
        assert!(matches!(time::parse_time("12"), Err(Error::Syntax)));
    }

    #[test]
    fn alarm_parser_parses_default_alarm() {
        let node = AlarmParser.handle("alarm: Wake up").unwrap().unwrap();

        match node {
            Node::Alarm(alarm) => {
                assert_eq!(alarm.name(), "Wake up");
                assert!(matches!(alarm.kind(), AlarmType::Default));
            }
            _ => panic!("expected alarm node"),
        }
    }

    #[test]
    fn alarm_parser_parses_explicit_kind() {
        let node = AlarmParser
            .handle("alarm: Wake up (important)")
            .unwrap()
            .unwrap();

        match node {
            Node::Alarm(alarm) => {
                assert_eq!(alarm.name(), "Wake up");
                assert!(matches!(alarm.kind(), AlarmType::Important));
            }
            _ => panic!("expected alarm node"),
        }
    }

    #[test]
    fn alarm_parser_unescapes_parentheses_in_title() {
        let node = AlarmParser
            .handle(r"alarm: Call Mom \(weekly\)")
            .unwrap()
            .unwrap();

        match node {
            Node::Alarm(alarm) => assert_eq!(alarm.name(), "Call Mom (weekly)"),
            _ => panic!("expected alarm node"),
        }
    }

    #[test]
    fn reference_parser_parses_checkbox_link() {
        let node = ReferenceParser.handle("- [ ] [[chores]]").unwrap().unwrap();

        match node {
            Node::Reference(reference) => assert_eq!(reference.target(), "chores.md"),
            _ => panic!("expected reference node"),
        }
    }

    #[test]
    fn once_parser_builds_non_repeating_schedule() {
        let node = OnceScheduler
            .handle("schedule: 15/7/2027 09:45")
            .unwrap()
            .unwrap();

        match node {
            Node::AlarmSchedule(schedule) => {
                assert!(matches!(schedule.repeat(), RepeatSpec::Never));
                let at = schedule.at().unwrap();
                assert_eq!(at.date().day, 15);
                assert_eq!(at.date().month, 7);
                assert_eq!(at.date().year, 2027);
                assert_eq!(at.time().hour, 9);
                assert_eq!(at.time().minute, 45);
            }
            _ => panic!("expected schedule node"),
        }
    }

    #[test]
    fn daily_parser_builds_daily_schedule() {
        let node = DailyParser
            .handle("schedule: daily 08:30")
            .unwrap()
            .unwrap();

        match node {
            Node::AlarmSchedule(schedule) => {
                assert!(matches!(schedule.repeat(), RepeatSpec::Daily));
                let at = schedule.at().unwrap();
                assert_eq!(at.time().hour, 8);
                assert_eq!(at.time().minute, 30);
            }
            _ => panic!("expected schedule node"),
        }
    }

    #[test]
    fn weekly_parser_builds_weekly_schedule() {
        let node = WeeklyParser
            .handle("schedule: weekly lunes viernes 06:15")
            .unwrap()
            .unwrap();

        match node {
            Node::AlarmSchedule(schedule) => match schedule.repeat() {
                RepeatSpec::Weekly(days) => {
                    assert!(days.contains(Weekday::Lunes));
                    assert!(days.contains(Weekday::Viernes));
                    assert!(!days.contains(Weekday::Martes));
                }
                _ => panic!("expected weekly repeat"),
            },
            _ => panic!("expected schedule node"),
        }
    }

    #[test]
    fn monthly_parser_builds_day_of_month_rule() {
        let node = MonthlyParser
            .handle("schedule: monthly 15 10:00")
            .unwrap()
            .unwrap();

        match node {
            Node::AlarmSchedule(schedule) => match schedule.repeat() {
                RepeatSpec::Monthly(MonthlyRule::DayOfMonth(day)) => assert_eq!(day, 15),
                _ => panic!("expected monthly day-of-month rule"),
            },
            _ => panic!("expected schedule node"),
        }
    }

    #[test]
    fn monthly_parser_builds_nth_weekday_rule() {
        let node = MonthlyParser
            .handle("schedule: monthly primer lunes 10:00")
            .unwrap()
            .unwrap();

        match node {
            Node::AlarmSchedule(schedule) => match schedule.repeat() {
                RepeatSpec::Monthly(MonthlyRule::NthWeekday(Weekday::Lunes, 1)) => {}
                _ => panic!("expected nth-weekday monthly rule"),
            },
            _ => panic!("expected schedule node"),
        }
    }

    #[test]
    fn yearly_parser_builds_day_month_rule() {
        let node = YearlyParser
            .handle("schedule: yearly 25/12 07:00")
            .unwrap()
            .unwrap();

        match node {
            Node::AlarmSchedule(schedule) => match schedule.repeat() {
                RepeatSpec::Yearly(YearlyRule::DayMonth(day, month)) => {
                    assert_eq!(day, 25);
                    assert_eq!(month, 12);
                }
                _ => panic!("expected yearly day/month rule"),
            },
            _ => panic!("expected schedule node"),
        }
    }
}
