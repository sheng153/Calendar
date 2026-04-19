#[derive(serde::Deserialize)]
struct AlarmKindConfig {
    #[serde(default)]
    pub media: Option<MediaConfig>,

    #[serde(default)]
    pub notification: Option<NotificationConfig>,
    #[serde(default)]
    pub snooze: Option<SnoozeConfig>,
}

fn default_audio_backend() -> AudioBackend {
    AudioBackend::Mpv
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaConfig {
    pub source: String,

    #[serde(default = "default_audio_backend")]
    pub backend: AudioBackend,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBackend {
    Mpv,
    XdgOpen,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub summary: Option<String>,

    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

fn default_true() -> bool {
    true
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnoozeConfig {
    pub count: u32,
    pub interval_seconds: u64,
}

#[derive(Debug)]
pub struct FireArgs {
    pub title: String,
    pub kind: String,
}

pub fn fire_main<I: IntoIterator<Item = std::ffi::OsString>>(args: I) -> Result<(), String> {
    let args = parse_fire_args(args)?;
    eprintln!("Alarm fired: title={:?}, kind={:?}", args.title, args.kind);

    let config_path = resolve_kind_path(&args.kind)?;
    let config = load_kind_config(&config_path)
        .map_err(|e| format!("failed to load kind config {}: {e}", config_path.display()))?;

    eprintln!("Loaded kind config: {}", config_path.display());

    run_alarm(&args.title, &config)
}

fn parse_fire_args<I: IntoIterator<Item = std::ffi::OsString>>(
    args: I,
) -> Result<FireArgs, String> {
    let mut title: Option<String> = None;
    let mut kind: Option<String> = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.to_string_lossy().as_ref() {
            "--title" => {
                let value = iter.next().ok_or("--title requires a value")?;
                title = Some(value.to_string_lossy().into_owned());
            }
            "--kind" => {
                let value = iter.next().ok_or("--kind requires a value")?;
                kind = Some(value.to_string_lossy().into_owned());
            }
            other => return Err(format!("unknown fire arg: {other}")),
        }
    }

    Ok(FireArgs {
        title: title.ok_or("missing --title")?,
        kind: kind.ok_or("missing --kind")?,
    })
}

fn run_alarm(title: &str, config: &AlarmKindConfig) -> Result<(), String> {
    let mut media = play_music_only(config).unwrap_or_else(|e| {
        eprintln!("Failed to start media {e}");
        MediaSession::none()
    });

    match ask_user(title, config)? {
        AlarmAction::Dismiss => {
            media.stop()?;
            return Ok(());
        }
        AlarmAction::Snooze => media.stop()?,
    }

    let Some(snooze) = &config.snooze else {
        return Ok(());
    };

    for _ in 1..snooze.count {
        std::thread::sleep(std::time::Duration::from_secs(snooze.interval_seconds));

        let mut media = play_music_only(config).unwrap_or_else(|e| {
            eprintln!("Failed to start media {e}");
            MediaSession::none()
        });

        match ask_user(title, config)? {
            AlarmAction::Dismiss => {
                media.stop()?;
                return Ok(());
            }
            AlarmAction::Snooze => {
                media.stop()?;
            }
        }
    }

    Ok(())
}

struct MediaSession {
    child: Option<std::process::Child>,
}

impl MediaSession {
    fn new(child: std::process::Child) -> Self {
        let child = Some(child);
        Self { child }
    }

    fn none() -> Self {
        Self { child: None }
    }

    fn stop(&mut self) -> Result<(), String> {
        let Some(child) = &mut self.child else {
            return Ok(());
        };

        child
            .kill()
            .map_err(|e| format!("Failed to stop child media process: {e}"))?;
        child
            .wait()
            .map_err(|e| format!("Failed to reap child media process: {e}"))?;

        self.child = None;

        Ok(())
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn play_music_only(config: &AlarmKindConfig) -> Result<MediaSession, String> {
    let Some(media) = &config.media else {
        return Ok(MediaSession::none());
    };

    let mut cmd = match media.backend {
        AudioBackend::Mpv => {
            let mut cmd = std::process::Command::new("mpv");
            cmd.arg("--no-video");
            cmd.arg("--loop-file=inf");
            cmd.arg("--really-quiet");
            cmd.arg("--ytdl-format=bestaudio");
            cmd.arg(&media.source);
            cmd
        }
        AudioBackend::XdgOpen => {
            let mut cmd = std::process::Command::new("xdg-open");
            cmd.arg(&media.source);
            cmd
        }
    };

    let child = cmd.spawn().map_err(|e| {
        format!(
            "failed to launch media with backend {:?} for {}: {e}",
            media.backend, media.source,
        )
    })?;

    Ok(MediaSession::new(child))
}

enum AlarmAction {
    Dismiss,
    Snooze,
}

fn ask_user(title: &str, config: &AlarmKindConfig) -> Result<AlarmAction, String> {
    let Some(notif) = config.notification.as_ref() else {
        return Ok(AlarmAction::Snooze);
    };

    if !notif.enabled {
        return Ok(AlarmAction::Snooze);
    }

    let summary = notif.summary.as_deref().unwrap_or("");
    let body = notif.body.as_deref().unwrap_or("");
    let text = match (summary.is_empty(), body.is_empty()) {
        (false, false) => format!("{summary}\n\n{body}"),
        (false, true) => summary.to_string(),
        (true, false) => body.to_string(),
        (true, true) => String::new(),
    };

    let status = std::process::Command::new("zenity")
        .arg("--question")
        .arg("--title")
        .arg(format!("Alarm: {title}"))
        .arg("--text")
        .arg(text)
        .arg("--ok-label")
        .arg("Dismiss")
        .arg("--cancel-label")
        .arg("Snooze")
        .status()
        .map_err(|e| format!("failed to launch zenity: {e}"))?;

    match status.code() {
        Some(0) => Ok(AlarmAction::Dismiss),
        Some(1) => Ok(AlarmAction::Snooze),
        Some(5) => Ok(AlarmAction::Snooze),
        other => Err(format!("zenity exited with unexpected status {other:?}")),
    }
}

fn resolve_kind_path(kind: &str) -> Result<std::path::PathBuf, String> {
    if kind.contains('/') || kind.contains('\\') || kind == "." || kind == ".." {
        return Err("invalid kind name".into());
    }

    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(std::path::Path::new(&home)
        .join(".config")
        .join("alarms")
        .join("kinds")
        .join(format!("{kind}.json")))
}

fn load_kind_config(path: &std::path::Path) -> std::io::Result<AlarmKindConfig> {
    let contents = std::fs::read_to_string(path)?;
    let config = serde_json::from_str(&contents)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::{parse_fire_args, resolve_kind_path};

    #[test]
    fn parse_fire_args_reads_title_and_kind() {
        let args = vec!["--title", "Wake up", "--kind", "important"]
            .into_iter()
            .map(std::ffi::OsString::from);

        let parsed = parse_fire_args(args).unwrap();

        assert_eq!(parsed.title, "Wake up");
        assert_eq!(parsed.kind, "important");
    }

    #[test]
    fn parse_fire_args_rejects_missing_title_value() {
        let args = vec!["--title"].into_iter().map(std::ffi::OsString::from);
        let err = parse_fire_args(args).unwrap_err();
        assert!(err.contains("--title requires a value"));
    }

    #[test]
    fn parse_fire_args_rejects_missing_kind() {
        let args = vec!["--title", "Wake up"]
            .into_iter()
            .map(std::ffi::OsString::from);

        let err = parse_fire_args(args).unwrap_err();
        assert!(err.contains("missing --kind"));
    }

    #[test]
    fn parse_fire_args_rejects_unknown_flag() {
        let args = vec!["--wat", "x"].into_iter().map(std::ffi::OsString::from);
        let err = parse_fire_args(args).unwrap_err();
        assert!(err.contains("unknown fire arg"));
    }

    #[test]
    fn resolve_kind_path_rejects_path_traversal_like_inputs() {
        assert!(resolve_kind_path("../secret").is_err());
        assert!(resolve_kind_path("a/b").is_err());
        assert!(resolve_kind_path(r"a\b").is_err());
        assert!(resolve_kind_path(".").is_err());
        assert!(resolve_kind_path("..").is_err());
    }

    #[test]
    fn resolve_kind_path_builds_expected_location() {
        unsafe {
            std::env::set_var("HOME", "/tmp/test-home");
        }

        let path = resolve_kind_path("important").unwrap();

        assert_eq!(
            path,
            std::path::Path::new("/tmp/test-home")
                .join(".config")
                .join("alarms")
                .join("kinds")
                .join("important.json")
        );
    }
}
