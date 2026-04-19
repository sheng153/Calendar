mod compile;
mod errors;
mod fire;
mod parser;
mod statics;
mod systemd;

use std::{env, fs, process};

fn main() {
    let mut args = env::args_os();
    let _program = args.next();

    match args.next() {
        Some(cmd) if cmd == "fire" => {
            if let Err(err) = fire::fire_main(args) {
                eprintln!("fire failed: {err}");
                process::exit(1);
            }
            process::exit(0);
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other:?}");
            process::exit(2);
        }
        None => {
            if let Err(err) = install_main() {
                eprintln!("install failed: {err}");
                process::exit(1);
            }
        }
    }
}

fn install_main() -> Result<(), String> {
    let root = statics::main_scheduler_path();
    let contents = match fs::read_to_string(&root) {
        Ok(constant) => constant,
        Err(error) => return Err(format!("failed to read {root:?}: {error:?}")),
    };

    let mut stack = vec![root.clone()];

    let (expanded_nodes, expanded_errors) =
        compile::expand_references(contents.as_str(), root.as_path(), &mut stack);

    if !expanded_errors.is_empty() {
        eprintln!("Input errors:");
        for error in &expanded_errors {
            eprintln!("{error}");
        }
        process::exit(1);
    }

    let compiled_alarms = compile::compile_alarms(&expanded_nodes);

    if !compiled_alarms.is_clean() {
        eprintln!("Compile errors:");
        for error in compiled_alarms.errors() {
            eprintln!("{error}");
        }
        process::exit(1);
    }

    let bin_path = match env::current_exe() {
        Ok(bin_path) => bin_path,
        Err(error) => return Err(format!("failed to locate current executable: {error:?}")),
    };

    let bin_path = bin_path.display().to_string();

    let rendered = systemd::render_alarms(compiled_alarms.alarms(), &bin_path);

    let unit_dir = statics::systemd_config_user();
    let written = match systemd::sync_units(&rendered, &unit_dir) {
        Ok(written) => written,
        Err(error) => return Err(format!("Failed to install units: {error}")),
    };

    if let Err(error) = systemd::daemon_reload_user() {
        return Err(format!("Failed to daemon reload: {error}"));
    }

    if let Err(error) = systemd::enable_timers(&rendered) {
        return Err(format!("Failed to daemon reload: {error}"));
    }

    for path in written {
        eprintln!("Wrote {}", path.display());
    }

    Ok(())
}
