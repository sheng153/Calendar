use std::fmt;

#[derive(Debug, Clone)]
pub enum Error {
    Syntax,
    Type,
    Compile(CompileError),
}

#[derive(Debug, Clone)]
pub enum CompileError {
    ScheduleWithoutAlarm,
    AlarmWithoutSchedule {
        alarm_title: String,
    },
    ReferenceNotFound {
        file_path: String,
    },
    ParseInReference {
        file_path: String,
        error: Box<LineError>,
    },
}

impl Error {
    pub fn no_alarm() -> Self {
        Self::Compile(CompileError::ScheduleWithoutAlarm)
    }

    pub fn alarm_no_schedule(alarm: impl Into<String>) -> Self {
        Self::Compile(CompileError::AlarmWithoutSchedule {
            alarm_title: alarm.into(),
        })
    }

    pub fn reference_not_found(reference: impl Into<String>) -> Self {
        Self::Compile(CompileError::ReferenceNotFound {
            file_path: reference.into(),
        })
    }

    pub fn parse_in_reference(reference: impl Into<String>, error: LineError) -> Self {
        Self::Compile(CompileError::ParseInReference {
            file_path: reference.into(),
            error: Box::new(error),
        })
    }
}

#[derive(Debug, Clone)]
pub struct LineError {
    index: usize,
    line: String,
    error: Error,
}

impl LineError {
    pub fn new(index: usize, line: String, error: Error) -> Self {
        Self { index, line, error }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Syntax => write!(f, "Syntax Error."),
            Error::Type => write!(f, "Attempted node method on incorrect type."),
            Error::Compile(compile_error) => write!(f, "Compile Error: {}", compile_error),
        }
    }
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.index, self.line, self.error)
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScheduleWithoutAlarm => write!(f, "Schedule Without Alarm."),
            Self::AlarmWithoutSchedule { alarm_title: title } => {
                write!(f, "Alarm Without Schedule: {title}")
            }
            Self::ReferenceNotFound { file_path: path } => {
                write!(f, "Reference Not Found: {path}")
            }
            Self::ParseInReference { file_path, error } => {
                write!(f, "Parse in Reference \n[file: {file_path}]\n{error}")
            }
        }
    }
}

impl LineError {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn line(&self) -> &str {
        &self.line
    }

    pub fn error(&self) -> &Error {
        &self.error
    }
}
