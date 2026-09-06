#![cfg(unix)]

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    io::{self, Read},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use assertr::prelude::*;
use serde_json::Value;

const PHASE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(90);
const PROOF_TIMEOUT: Duration = Duration::from_secs(10);
static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
enum Signal {
    Interrupt,
    Terminate,
}

impl Signal {
    const fn number(self) -> libc::c_int {
        match self {
            Self::Interrupt => libc::SIGINT,
            Self::Terminate => libc::SIGTERM,
        }
    }

    const fn exit_code(self) -> i32 {
        match self {
            Self::Interrupt => 130,
            Self::Terminate => 143,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Interrupt => "SIGINT",
            Self::Terminate => "SIGTERM",
        }
    }
}

#[derive(Clone, Copy)]
enum Phase {
    ApplicationStartup,
    ActiveBrowserCallback,
}

impl Phase {
    const fn event(self) -> &'static str {
        match self {
            Self::ApplicationStartup => "app_starting",
            Self::ActiveBrowserCallback => "browser_active",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::ApplicationStartup => "application-startup",
            Self::ActiveBrowserCallback => "active-browser-callback",
        }
    }
}

#[derive(Clone, Copy)]
struct Scenario {
    signal: Signal,
    phase: Phase,
    repeated_signal: Option<Signal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProcessIdentity {
    pid: u32,
    start_time: String,
}

#[derive(Debug)]
struct ProcessInfo {
    identity: ProcessIdentity,
    parent_pid: u32,
}

#[test]
fn browser_test_signals() -> Result<(), Box<dyn Error>> {
    let executable = build_browser_test_executable()?;
    let scenarios = [
        Scenario {
            signal: Signal::Interrupt,
            phase: Phase::ApplicationStartup,
            repeated_signal: None,
        },
        Scenario {
            signal: Signal::Terminate,
            phase: Phase::ApplicationStartup,
            repeated_signal: None,
        },
        Scenario {
            signal: Signal::Interrupt,
            phase: Phase::ActiveBrowserCallback,
            repeated_signal: None,
        },
        Scenario {
            signal: Signal::Terminate,
            phase: Phase::ActiveBrowserCallback,
            repeated_signal: None,
        },
        Scenario {
            signal: Signal::Interrupt,
            phase: Phase::ActiveBrowserCallback,
            repeated_signal: Some(Signal::Terminate),
        },
    ];

    for scenario in scenarios {
        eprintln!(
            "running {} during {}{}",
            scenario.signal.label(),
            scenario.phase.label(),
            if scenario.repeated_signal.is_some() {
                " with a repeated signal"
            } else {
                ""
            }
        );
        run_scenario(&executable, scenario)?;
        eprintln!(
            "passed {} during {}",
            scenario.signal.label(),
            scenario.phase.label()
        );
    }
    Ok(())
}

fn build_browser_test_executable() -> Result<PathBuf, Box<dyn Error>> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let output = Command::new(cargo_program())
        .args([
            "build",
            "--manifest-path",
            manifest.to_str().ok_or("manifest path is not Unicode")?,
            "--features",
            "browser-tests",
            "--test",
            "browser_test",
            "--message-format=json-render-diagnostics",
        ])
        .output()?;
    assert_that!(output.status.success())
        .with_detail_message(String::from_utf8_lossy(&output.stderr).into_owned())
        .is_true();

    let executable = String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|message| {
            let target = message.get("target")?;
            (message.get("reason")?.as_str()? == "compiler-artifact"
                && target.get("name")?.as_str()? == "browser_test")
                .then(|| message.get("executable")?.as_str().map(PathBuf::from))
                .flatten()
        })
        .ok_or("Cargo did not report the browser_test executable")?;
    assert_that!(executable.is_file()).is_true();
    Ok(executable)
}

fn run_scenario(executable: &Path, scenario: Scenario) -> Result<(), Box<dyn Error>> {
    let run_dir = tempfile::tempdir()?;
    let metadata_path = run_dir.path().join("probe.jsonl");
    let marker = unique_marker(scenario);
    let ports = allocate_ports()?;
    let mut child = KillChildOnDrop(spawn_browser_test(
        executable,
        &metadata_path,
        &marker,
        ports,
    )?);
    let mut stdout = Some(drain(
        child
            .0
            .stdout
            .take()
            .ok_or("browser test stdout was not piped")?,
    ));
    let mut stderr = Some(drain(
        child
            .0
            .stderr
            .take()
            .ok_or("browser test stderr was not piped")?,
    ));

    if let Err(err) = wait_for_event(
        &mut child.0,
        &metadata_path,
        scenario.phase.event(),
        PHASE_TIMEOUT,
    ) {
        let output = collect_output(
            stdout.take().expect("stdout reader must exist"),
            stderr.take().expect("stderr reader must exist"),
        )?;
        return Err(io::Error::new(err.kind(), format!("{err}\n\n{output}")).into());
    }
    let identities = descendant_identities(child.0.id())?;
    assert_that!(identities.is_empty())
        .with_detail_message("the exact browser-test process identity must be observable")
        .is_false();

    send_signal(child.0.id(), scenario.signal)?;
    if let Some(repeated_signal) = scenario.repeated_signal {
        wait_for_signal_event(&mut child.0, &metadata_path, scenario.signal, PHASE_TIMEOUT)?;
        send_signal(child.0.id(), repeated_signal)?;
        wait_for_signal_event(
            &mut child.0,
            &metadata_path,
            repeated_signal,
            CLEANUP_TIMEOUT,
        )?;
    }

    let status = wait_for_exit(&mut child.0, CLEANUP_TIMEOUT)?;
    let output = collect_output(
        stdout.take().expect("stdout reader must exist"),
        stderr.take().expect("stderr reader must exist"),
    )?;
    assert_that!(status.code())
        .with_detail_message(output.clone())
        .is_equal_to(Some(scenario.signal.exit_code()));

    let events = read_events(&metadata_path)?;
    assert_scenario_events(&events, scenario, &output);
    assert_temp_dir_removed(&events, &output);
    assert_identities_gone(&identities, &output)?;
    assert_ports_closed(ports, &output);
    assert_marker_gone(&marker, &output)?;
    Ok(())
}

struct KillChildOnDrop(Child);

impl Drop for KillChildOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _kill_result = self.0.kill();
            let _wait_result = self.0.wait();
        }
    }
}

fn spawn_browser_test(
    executable: &Path,
    metadata_path: &Path,
    marker: &str,
    ports: [u16; 4],
) -> io::Result<Child> {
    Command::new(executable)
        .args([
            "--exact",
            "browser_tests",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("DISPATCH_BROWSER_SIGNAL_SCENARIO", "1")
        .env("DISPATCH_BROWSER_TEST_METADATA", metadata_path)
        .env("DISPATCH_BROWSER_TEST_RUN_MARKER", marker)
        .env("DISPATCH_BROWSER_TEST_APP_PORT", ports[0].to_string())
        .env("DISPATCH_BROWSER_TEST_RELOAD_PORT", ports[1].to_string())
        .env("DISPATCH_BROWSER_TEST_WEBDRIVER_PORT", ports[2].to_string())
        .env("DISPATCH_BROWSER_TEST_DEVTOOLS_PORT", ports[3].to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

fn drain(mut reader: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        let _read_result = reader.read_to_end(&mut output);
        output
    })
}

fn wait_for_event(
    child: &mut Child,
    path: &Path,
    event: &str,
    timeout: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if read_events(path)?
            .iter()
            .any(|record| record.get("event").and_then(Value::as_str) == Some(event))
        {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "browser-test child exited with {status} before probe event {event:?}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out waiting for probe event {event:?}"),
    ))
}

fn collect_output(
    stdout: thread::JoinHandle<Vec<u8>>,
    stderr: thread::JoinHandle<Vec<u8>>,
) -> io::Result<String> {
    let stdout = String::from_utf8_lossy(&stdout.join().map_err(thread_join_error)?).into_owned();
    let stderr = String::from_utf8_lossy(&stderr.join().map_err(thread_join_error)?).into_owned();
    Ok(format!("stdout:\n{stdout}\n\nstderr:\n{stderr}"))
}

fn wait_for_signal_event(
    child: &mut Child,
    path: &Path,
    signal: Signal,
    timeout: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if read_events(path)?.iter().any(|record| {
            record.get("event").and_then(Value::as_str) == Some("signal_received")
                && record.pointer("/details/signal").and_then(Value::as_str) == Some(signal.label())
        }) {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "browser-test child exited with {status} before recording {}",
                signal.label()
            )));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out waiting for {} probe event", signal.label()),
    ))
}

fn read_events(path: &Path) -> io::Result<Vec<Value>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    Ok(contents
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

fn send_signal(pid: u32, signal: Signal) -> io::Result<()> {
    let pid = libc::pid_t::try_from(pid).map_err(|_| io::Error::other("pid does not fit pid_t"))?;
    // SAFETY: `pid` is the exact child id returned by `std::process::Child`; `signal` is SIGINT or
    // SIGTERM. The return value is checked immediately.
    if unsafe { libc::kill(pid, signal.number()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(50));
    }
    child.kill()?;
    let _status = child.wait()?;
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "browser-test cleanup exceeded its overall bound",
    ))
}

fn allocate_ports() -> io::Result<[u16; 4]> {
    let listeners = (0..4)
        .map(|_| TcpListener::bind(("127.0.0.1", 0)))
        .collect::<Result<Vec<_>, _>>()?;
    let ports = [
        listeners[0].local_addr()?.port(),
        listeners[1].local_addr()?.port(),
        listeners[2].local_addr()?.port(),
        listeners[3].local_addr()?.port(),
    ];
    drop(listeners);
    Ok(ports)
}

fn descendant_identities(root_pid: u32) -> io::Result<Vec<ProcessIdentity>> {
    let processes = process_table()?;
    let mut descendants = HashSet::from([root_pid]);
    loop {
        let before = descendants.len();
        for process in &processes {
            if descendants.contains(&process.parent_pid) {
                descendants.insert(process.identity.pid);
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    Ok(processes
        .into_iter()
        .filter(|process| descendants.contains(&process.identity.pid))
        .map(|process| process.identity)
        .collect())
}

fn process_table() -> io::Result<Vec<ProcessInfo>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,lstart=,command="])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            "ps failed while reading process identities",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_process_line)
        .collect())
}

fn parse_process_line(line: &str) -> Option<ProcessInfo> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 8 {
        return None;
    }
    Some(ProcessInfo {
        identity: ProcessIdentity {
            pid: fields[0].parse().ok()?,
            start_time: fields[2..7].join(" "),
        },
        parent_pid: fields[1].parse().ok()?,
    })
}

fn assert_scenario_events(events: &[Value], scenario: Scenario, output: &str) {
    let browser_active = events
        .iter()
        .any(|record| record.get("event").and_then(Value::as_str) == Some("browser_active"));
    match scenario.phase {
        Phase::ApplicationStartup => {
            assert_that!(browser_active)
                .with_detail_message(output.to_owned())
                .is_false();
        }
        Phase::ActiveBrowserCallback => {
            assert_that!(browser_active)
                .with_detail_message(output.to_owned())
                .is_true();
        }
    }

    let signal_events = events
        .iter()
        .filter(|record| record.get("event").and_then(Value::as_str) == Some("signal_received"))
        .count();
    let expected = usize::from(scenario.repeated_signal.is_some()) + 1;
    assert_that!(signal_events)
        .with_detail_message(output.to_owned())
        .is_equal_to(expected);
}

fn assert_temp_dir_removed(events: &[Value], output: &str) {
    let temp_dir = events
        .iter()
        .find(|record| record.get("event").and_then(Value::as_str) == Some("app_starting"))
        .and_then(|record| record.pointer("/details/temp_dir"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    assert_that!(temp_dir.is_some())
        .with_detail_message(output.to_owned())
        .is_true();
    assert_that!(temp_dir.is_some_and(|path| path.exists()))
        .with_detail_message(output.to_owned())
        .is_false();
}

fn assert_identities_gone(identities: &[ProcessIdentity], output: &str) -> io::Result<()> {
    let deadline = Instant::now() + PROOF_TIMEOUT;
    loop {
        let current = process_table()?
            .into_iter()
            .map(|process| (process.identity.pid, process.identity.start_time))
            .collect::<HashMap<_, _>>();
        let live = identities
            .iter()
            .any(|identity| current.get(&identity.pid) == Some(&identity.start_time));
        if !live || Instant::now() >= deadline {
            assert_that!(live)
                .with_detail_message(output.to_owned())
                .is_false();
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn assert_ports_closed(ports: [u16; 4], output: &str) {
    for port in ports {
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let open = TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok();
        assert_that!(open)
            .with_detail_message(format!("port {port} remained open\n{output}"))
            .is_false();
    }
}

fn assert_marker_gone(marker: &str, output: &str) -> io::Result<()> {
    let deadline = Instant::now() + PROOF_TIMEOUT;
    loop {
        let ps = Command::new("ps")
            .args(["eww", "-axo", "pid=,command="])
            .output()?;
        let remains = String::from_utf8_lossy(&ps.stdout)
            .lines()
            .any(|line| line.contains(marker));
        if !remains || Instant::now() >= deadline {
            assert_that!(remains)
                .with_detail_message(output.to_owned())
                .is_false();
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn unique_marker(scenario: Scenario) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "dispatch-browser-signal-{}-{}-{nanos}-{sequence}",
        scenario.phase.label(),
        scenario.signal.label()
    )
}

fn cargo_program() -> std::ffi::OsString {
    env!("CARGO").into()
}

fn thread_join_error(_payload: Box<dyn std::any::Any + Send>) -> io::Error {
    io::Error::other("browser-test output reader thread panicked")
}
