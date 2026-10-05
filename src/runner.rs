use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const UPSTREAM_REV: &str = "4fd4f59c9a442e6414c20ae3fd364a082e97f1ff";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub engine: PathBuf,
    pub output: PathBuf,
    pub json: bool,
    pub fasta: bool,
    pub frequency: bool,
    pub disable_linear_sweep: bool,
    pub disable_pattern_search: bool,
    pub disable_symbol_parsing: bool,
    pub disable_fs_parsing: bool,
    pub disable_got_parsing: bool,
    pub disable_opd_parsing: bool,
    pub prefix: String,
    pub function_names: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: find_engine().unwrap_or_default(),
            output: default_output(),
            json: true,
            fasta: false,
            frequency: false,
            disable_linear_sweep: false,
            disable_pattern_search: false,
            disable_symbol_parsing: false,
            disable_fs_parsing: false,
            disable_got_parsing: false,
            disable_opd_parsing: false,
            prefix: "default".into(),
            function_names: false,
        }
    }
}

fn default_output() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    home.map(PathBuf::from)
        .map(|p| p.join("Documents").join("RADD Results"))
        .unwrap_or_else(|| std::env::temp_dir().join("RADD Results"))
}

pub fn find_engine() -> Option<PathBuf> {
    let name = if cfg!(windows) { "radd.exe" } else { "radd" };
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
            candidates.push(dir.join("tools").join(name));
        }
    }
    if let Ok(dir) = std::env::current_dir() {
        candidates.push(dir.join("tools").join(name));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|p| p.join(name)));
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub fn validate(
    settings: &Settings,
    inputs: &[PathBuf],
    config: &Option<PathBuf>,
) -> Result<(), String> {
    if !settings.engine.is_file() {
        return Err("Choose the RADD engine first (radd.exe on Windows, radd on Linux/macOS). See the Guide for setup.".into());
    }
    if inputs.is_empty() {
        return Err("Add at least one binary to analyze.".into());
    }
    for path in inputs {
        if !path.is_file() {
            return Err(format!("Input file is missing: {}", path.display()));
        }
    }
    // RADD names outputs using file basenames, so duplicates can overwrite one another.
    let mut names = std::collections::HashSet::new();
    for path in inputs {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if !names.insert(name) {
            return Err("Two inputs have the same filename. Analyze them in separate runs to keep their results distinct.".into());
        }
    }
    if let Some(path) = config {
        if inputs.len() != 1 {
            return Err("A header configuration can only be used with one input file.".into());
        }
        let data = fs::read(path).map_err(|e| format!("Cannot read header configuration: {e}"))?;
        serde_json::from_slice::<serde_json::Value>(&data)
            .map_err(|e| format!("Header configuration is not valid JSON: {e}"))?;
    }
    if settings.output.as_os_str().is_empty() {
        return Err("Choose a results folder.".into());
    }
    if settings.output.exists() && !settings.output.is_dir() {
        return Err("The results path is a file. Choose a folder.".into());
    }
    Ok(())
}

pub fn arguments(
    settings: &Settings,
    inputs: &[PathBuf],
    config: &Option<PathBuf>,
    run_dir: &Path,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("--disasm-out"), run_dir.as_os_str().into()];
    for (enabled, flag) in [
        (settings.json, "--json-out"),
        (settings.fasta, "--fasta-out"),
        (settings.frequency, "--fasta-freq"),
    ] {
        if enabled {
            args.extend([OsString::from(flag), run_dir.as_os_str().into()]);
        }
    }
    if let Some(path) = config {
        args.extend([OsString::from("--config-file"), path.as_os_str().into()]);
    }
    for (enabled, flag) in [
        (settings.disable_linear_sweep, "--disable-linear-sweep"),
        (settings.disable_pattern_search, "--disable-pattern-search"),
        (settings.disable_symbol_parsing, "--disable-symbol-parsing"),
        (settings.disable_fs_parsing, "--disable-fs-parsing"),
        (settings.disable_got_parsing, "--disable-got-parsing"),
        (settings.disable_opd_parsing, "--disable-opd-parsing"),
        (
            settings.function_names,
            "--fasta-sequence-use-function-name",
        ),
    ] {
        if enabled {
            args.push(flag.into());
        }
    }
    args.extend([
        OsString::from("--fasta-prefix"),
        OsString::from(&settings.prefix),
    ]);
    args.push("--".into());
    args.extend(inputs.iter().map(|p| p.as_os_str().into()));
    args
}

pub struct Job {
    pub receiver: Receiver<Result<String, String>>,
    pub cancel: Arc<AtomicBool>,
    pub directory: PathBuf,
    pub started: std::time::Instant,
    worker: Option<thread::JoinHandle<()>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Join before process exit so closing the GUI cannot orphan the engine.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn new_directory(base: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(base).map_err(|e| format!("Cannot create results folder: {e}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for n in 0..1000 {
        let dir = base.join(format!("run-{stamp}-{n}"));
        match fs::create_dir(&dir) {
            Ok(()) => return std::path::absolute(dir).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create analysis folder: {e}")),
        }
    }
    Err("Could not allocate a unique results folder.".into())
}

pub fn start(
    settings: Settings,
    inputs: Vec<PathBuf>,
    config: Option<PathBuf>,
    check: bool,
) -> Result<Job, String> {
    if !check {
        validate(&settings, &inputs, &config)?;
    }
    if !settings.engine.is_file() {
        return Err("Select an existing RADD executable first.".into());
    }
    let directory = new_directory(&settings.output)?;
    let args = if check {
        vec![OsString::from("--arches")]
    } else {
        arguments(&settings, &inputs, &config, &directory)
    };
    let manifest = serde_json::json!({
        "engine": settings.engine, "arguments": args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>(),
        "inputs": inputs, "header_config": config, "settings": settings,
        "compatible_upstream_revision": UPSTREAM_REV, "operation": if check { "check-engine" } else { "analysis" }
    });
    fs::write(
        directory.join("run.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let stdout = File::create(directory.join("engine.log")).map_err(|e| e.to_string())?;
    let stderr = stdout.try_clone().map_err(|e| e.to_string())?;
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let folder = directory.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let mut command = Command::new(&settings.engine);
            command
                .args(&args)
                .stdin(Stdio::null())
                .stdout(stdout)
                .stderr(stderr)
                .env("NO_COLOR", "1")
                .env("RUST_MIN_STACK", "8388608");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }
            let mut child = command.spawn().map_err(|e| {
                format!(
                    "Could not start RADD: {e}. Check the engine path and executable permissions."
                )
            })?;
            let status = loop {
                if stop.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Cancelled. Any files already written are partial results.".into());
                }
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => thread::sleep(Duration::from_millis(100)),
                    Err(e) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("Cannot monitor RADD: {e}"));
                    }
                }
            };
            if !status.success() {
                return Err(format!(
                    "RADD exited with {status}. Read the Engine log below for details."
                ));
            }
            if check {
                return Ok("Engine responded successfully. Supported architectures appear in the Engine log.".into());
            }
            let files = result_files(&folder);
            for (required, suffix) in [
                (true, ".asm"),
                (settings.json, ".json"),
                (settings.fasta, ".fasta"),
                (settings.frequency, ".freq.json"),
            ] {
                if required
                    && !files.iter().any(|p| {
                        let name = p.file_name().unwrap_or_default().to_string_lossy();
                        name.ends_with(suffix)
                            && (suffix != ".json" || !name.ends_with(".freq.json"))
                            && fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
                    })
                {
                    return Err(format!(
                        "RADD exited successfully but did not produce the requested {suffix} output. Inspect the log; the input may be unsupported."
                    ));
                }
            }
            Ok(format!(
                "Analysis complete. {} output files saved. Review the engine log for warnings about skipped inputs.",
                files.len()
            ))
        })();
        let _ = fs::write(
            folder.join("status.txt"),
            match &result {
                Ok(s) => s.as_str(),
                Err(s) => s.as_str(),
            },
        );
        let _ = sender.send(result);
    });
    Ok(Job {
        receiver,
        cancel,
        directory,
        started: std::time::Instant::now(),
        worker: Some(worker),
    })
}

pub fn result_files(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && !matches!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("run.json" | "engine.log" | "status.txt")
                )
        })
        .collect();
    paths.sort();
    paths
}

pub fn read_preview(path: &Path, tail: bool) -> String {
    let result = (|| -> std::io::Result<String> {
        let mut file = File::open(path)?;
        let size = file.metadata()?.len();
        let limit = if tail { 32_768 } else { 262_144 };
        if tail && size > limit {
            file.seek(SeekFrom::End(-(limit as i64)))?;
        }
        let mut bytes = Vec::new();
        file.take(limit).read_to_end(&mut bytes)?;
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if size > limit {
            text.push_str("\n\n[Preview limited. The full file is saved in the results folder.]");
        }
        Ok(text)
    })();
    result.unwrap_or_else(|e| format!("Cannot read {}: {e}", path.display()))
}

pub fn open_folder(path: &Path) -> Result<(), String> {
    let mut cmd = Command::new(if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    });
    cmd.arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open folder: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arguments_preserve_paths_and_match_upstream() {
        let s = Settings {
            json: true,
            fasta: true,
            frequency: true,
            disable_linear_sweep: true,
            ..Settings::default()
        };
        let args = arguments(
            &s,
            &[PathBuf::from("/a path/-input;name")],
            &Some(PathBuf::from("/config file.json")),
            Path::new("/results here"),
        );
        assert!(
            args.windows(2)
                .any(|a| a == ["--config-file", "/config file.json"])
        );
        assert!(
            args.windows(2)
                .any(|a| a == ["--disasm-out", "/results here"])
        );
        assert!(args.contains(&OsString::from("--disable-linear-sweep")));
        assert!(args.windows(2).any(|a| a == ["--", "/a path/-input;name"]));
        assert!(args.contains(&OsString::from("--fasta-freq")));
    }
    #[test]
    fn run_directories_never_reuse_existing_output() {
        let base = std::env::temp_dir().join(format!("radd-gui-test-{}", std::process::id()));
        let a = new_directory(&base).unwrap();
        let b = new_directory(&base).unwrap();
        assert_ne!(a, b);
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn config_and_duplicate_inputs_are_rejected() {
        let exe = std::env::current_exe().unwrap();
        let s = Settings {
            engine: exe.clone(),
            ..Settings::default()
        };
        assert!(
            validate(&s, &[], &None)
                .unwrap_err()
                .contains("at least one")
        );
        assert!(
            validate(&s, &[exe.clone(), exe.clone()], &None)
                .unwrap_err()
                .contains("same filename")
        );
        assert!(
            validate(&s, &[exe], &Some(PathBuf::from("missing-config.json")))
                .unwrap_err()
                .contains("Cannot read")
        );
    }

    #[test]
    #[ignore = "Requires RADD_TEST_ENGINE and RADD_TEST_INPUT pointing to a built engine and supported binary"]
    fn real_engine() {
        let engine =
            PathBuf::from(std::env::var_os("RADD_TEST_ENGINE").expect("Set RADD_TEST_ENGINE"));
        let input =
            PathBuf::from(std::env::var_os("RADD_TEST_INPUT").expect("Set RADD_TEST_INPUT"));
        let root = std::env::temp_dir().join(format!("radd integration {}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let spaced_input = root.join("binary with spaces");
        fs::copy(&input, &spaced_input).unwrap();
        let settings = Settings {
            engine,
            output: root.join("output with spaces"),
            json: true,
            fasta: true,
            frequency: true,
            ..Settings::default()
        };
        let check = start(settings.clone(), vec![], None, true).unwrap();
        check
            .receiver
            .recv_timeout(Duration::from_secs(120))
            .unwrap()
            .unwrap();
        assert!(
            read_preview(&check.directory.join("engine.log"), false)
                .contains("Supported architecture")
        );
        let job = start(settings.clone(), vec![spaced_input.clone()], None, false).unwrap();
        let outcome = job.receiver.recv_timeout(Duration::from_secs(120)).unwrap();
        if let Err(error) = outcome {
            panic!(
                "{error}\n{}",
                read_preview(&job.directory.join("engine.log"), false)
            );
        }
        let files = result_files(&job.directory);
        assert_eq!(files.len(), 4);
        for file in &files {
            assert!(fs::metadata(file).unwrap().len() > 0);
            if file.extension().is_some_and(|e| e == "json") {
                serde_json::from_slice::<serde_json::Value>(&fs::read(file).unwrap()).unwrap();
            }
        }
        let cancelled = start(settings.clone(), vec![spaced_input], None, false).unwrap();
        cancelled.cancel.store(true, Ordering::Relaxed);
        assert!(
            cancelled
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap_err()
                .contains("Cancelled")
        );
        let closing = start(settings.clone(), vec![input], None, false).unwrap();
        let closed_dir = closing.directory.clone();
        drop(closing);
        assert!(
            fs::read_to_string(closed_dir.join("status.txt"))
                .unwrap()
                .contains("Cancelled")
        );
        let bad = root.join("unsupported.bin");
        fs::write(&bad, b"not a binary header").unwrap();
        let failed = start(settings, vec![bad], None, false).unwrap();
        assert!(
            failed
                .receiver
                .recv_timeout(Duration::from_secs(120))
                .unwrap()
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
