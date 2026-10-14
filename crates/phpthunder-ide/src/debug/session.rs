//! A debug session: runs a PHP command under Xdebug (through the project's
//! interpreter, translating paths for containers), or listens for the
//! connections of web requests, and drives each from UI commands,
//! reporting events back.

use super::*;
use crate::interpreter::Interpreter;
use crate::tr;
use iced::futures::Stream;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Child;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// How long to wait for Xdebug to connect after the process starts.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Herd Pro's bundled Xdebug builds, tried when PHP has none loaded.
const HERD_XDEBUG_DIR: &str = "/Applications/Herd.app/Contents/Resources/xdebug";

/// What a session debugs.
#[derive(Debug, Clone)]
pub enum DebugTarget {
    /// Runs `php` with these arguments (e.g. `vendor/bin/pest tests/UserTest.php
    /// --filter …`, or a script) and debugs it.
    Run(Vec<OsString>),
    /// Listens on this port for Xdebug connections from web requests
    /// (`XDEBUG_SESSION` cookie or `xdebug.start_with_request`), one at a
    /// time, until stopped.
    Listen(u16),
}

/// What to run under the debugger.
#[derive(Debug, Clone)]
pub struct DebugLaunch {
    /// Working directory (the project root).
    pub root: PathBuf,
    /// The command to run, or the port to listen on.
    pub target: DebugTarget,
    /// Line breakpoints: file and 0-based line.
    pub breakpoints: Vec<(PathBuf, usize)>,
    /// Stop at the first line of the script instead of running to the
    /// first breakpoint.
    pub break_at_first_line: bool,
    /// Where PHP runs; paths are translated for it.
    pub interpreter: Interpreter,
}

/// Translates paths between the project on this machine and the
/// interpreter's view of it.
struct PathMap<'a> {
    /// Project root on this machine.
    root: &'a Path,
    /// The project's interpreter.
    interpreter: &'a Interpreter,
}

impl PathMap<'_> {
    /// A local path as the interpreter sees it.
    fn remote(&self, local: &Path) -> PathBuf {
        PathBuf::from(self.interpreter.to_remote(self.root, local))
    }

    /// A path reported by Xdebug, on this machine.
    fn local(&self, remote: &Path) -> PathBuf {
        self.interpreter
            .to_local(self.root, &remote.to_string_lossy())
    }
}

/// Starts a session in the background. Returns the command sender for the
/// UI and the stream of events it produces; the stream ends after
/// [`DebugEvent::Finished`].
pub fn start_session(
    launch: DebugLaunch,
) -> (
    UnboundedSender<DebugCommand>,
    impl Stream<Item = DebugEvent> + Send + 'static,
) {
    let (commands, command_rx) = unbounded_channel();
    let (events, event_rx) = unbounded_channel();
    tokio::spawn(async move {
        let error = run_session(launch, command_rx, events.clone()).await.err();
        let _ = events.send(DebugEvent::Finished(error));
    });
    let stream = iced::futures::stream::unfold(event_rx, |mut rx| async move {
        rx.recv().await.map(|event| (event, rx))
    });
    (commands, stream)
}

/// `-dzend_extension=…` for Xdebug when PHP does not load it already:
/// `PHPTHUNDER_XDEBUG`, else Herd Pro's build for this PHP version.
///
/// # Errors
/// Returns a message when no Xdebug can be found.
pub(crate) async fn xdebug_arguments(
    root: &Path,
    interpreter: &Interpreter,
) -> Result<Vec<String>, String> {
    let probe = |extra: Vec<String>| async move {
        let mut args: Vec<OsString> = extra.into_iter().map(OsString::from).collect();
        args.extend([
            "-r".into(),
            "echo extension_loaded('xdebug') ? 'yes' : PHP_MAJOR_VERSION . PHP_MINOR_VERSION;"
                .into(),
        ]);
        interpreter
            .command(root, "php", &args)
            .output()
            .await
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .map_err(|e| tr!("error-run-php", error = e.to_string()))
    };
    let loaded = probe(Vec::new()).await?;
    if loaded == "yes" {
        return Ok(Vec::new());
    }
    // Herd's builds only exist on this machine.
    if !interpreter.is_local() {
        return Err(tr!("debug-no-xdebug"));
    }
    let mut candidates: Vec<PathBuf> = std::env::var_os("PHPTHUNDER_XDEBUG")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    if let Ok(dir) = std::fs::read_dir(HERD_XDEBUG_DIR) {
        let prefix = format!("xdebug-{loaded}");
        candidates.extend(dir.filter_map(Result::ok).map(|e| e.path()).filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
        }));
    }
    for candidate in candidates {
        let args = vec![format!("-dzend_extension={}", candidate.display())];
        if probe(args.clone()).await? == "yes" {
            return Ok(args);
        }
    }
    Err(tr!("debug-no-xdebug"))
}

/// Forwards each line a child stream prints as [`DebugEvent::Output`].
fn forward_lines<R>(reader: R, events: UnboundedSender<DebugEvent>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if events.send(DebugEvent::Output(line)).is_err() {
                break;
            }
        }
    });
}

/// Runs the whole session: for a command, start it under Xdebug, accept
/// its connection and serve it; when listening, serve each incoming
/// connection in turn until the UI stops.
///
/// # Errors
/// Returns a message when Xdebug is missing, the process cannot start,
/// the port is taken, or Xdebug never connects.
async fn run_session(
    launch: DebugLaunch,
    mut commands: UnboundedReceiver<DebugCommand>,
    events: UnboundedSender<DebugEvent>,
) -> Result<(), String> {
    let map = PathMap {
        root: &launch.root,
        interpreter: &launch.interpreter,
    };
    let first = if launch.break_at_first_line {
        "step_into"
    } else {
        "run"
    };
    match &launch.target {
        DebugTarget::Run(program) => {
            run_program(&launch, program, &map, first, &mut commands, &events).await
        }
        DebugTarget::Listen(port) => {
            listen(
                *port,
                launch.breakpoints.clone(),
                &map,
                first,
                &mut commands,
                &events,
            )
            .await
        }
    }
}

/// Starts `program` under Xdebug and serves its connection.
///
/// # Errors
/// See [`run_session`].
async fn run_program(
    launch: &DebugLaunch,
    program: &[OsString],
    map: &PathMap<'_>,
    first: &'static str,
    commands: &mut UnboundedReceiver<DebugCommand>,
    events: &UnboundedSender<DebugEvent>,
) -> Result<(), String> {
    let interpreter = &launch.interpreter;
    // A container reaches this machine through Docker's host alias.
    let (bind, client_host) = match interpreter {
        Interpreter::Local { .. } => ("127.0.0.1", "127.0.0.1"),
        Interpreter::Docker { .. } | Interpreter::Compose { .. } => {
            ("0.0.0.0", "host.docker.internal")
        }
        Interpreter::Ssh { .. } => return Err(tr!("debug-ssh-unsupported")),
    };
    let xdebug = xdebug_arguments(&launch.root, interpreter).await?;
    let listener = TcpListener::bind((bind, 0))
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let mut args: Vec<OsString> = xdebug.into_iter().map(OsString::from).collect();
    args.extend([
        "-dxdebug.mode=debug".into(),
        "-dxdebug.start_with_request=yes".into(),
        format!("-dxdebug.client_host={client_host}").into(),
        format!("-dxdebug.client_port={port}").into(),
    ]);
    args.extend(program.iter().cloned());
    let mut child = interpreter
        .command(&launch.root, "php", &args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| tr!("error-run-php", error = e.to_string()))?;
    if let Some(out) = child.stdout.take() {
        forward_lines(out, events.clone());
    }
    if let Some(err) = child.stderr.take() {
        forward_lines(err, events.clone());
    }
    let stream = tokio::select! {
        accepted = tokio::time::timeout(CONNECT_TIMEOUT, listener.accept()) => match accepted {
            Ok(Ok((stream, _))) => stream,
            _ => return Err(tr!("debug-no-connection")),
        },
        _ = child.wait() => return Err(tr!("debug-no-connection")),
        command = commands.recv() => {
            // Stopped (or the UI went away) before Xdebug connected.
            if matches!(command, Some(DebugCommand::Stop) | None) {
                let _ = child.kill().await;
                return Ok(());
            }
            return Err(tr!("debug-no-connection"));
        }
    };
    // Later connections (child processes) are refused, so they run freely.
    drop(listener);
    let mut connection = Connection::new(stream);
    drive(
        &mut connection,
        launch.breakpoints.clone(),
        commands,
        events,
        Some(&mut child),
        map,
        first,
    )
    .await?;
    // Let the script finish without stopping at more breakpoints.
    let _ = connection.call("detach", "", None).await;
    if tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
    }
    Ok(())
}

/// Listens on `port` and serves each Xdebug connection (a web request)
/// in turn; breakpoints set while waiting apply to the next one.
///
/// # Errors
/// Returns a message when the port cannot be opened.
async fn listen(
    port: u16,
    mut breakpoints: Vec<(PathBuf, usize)>,
    map: &PathMap<'_>,
    first: &'static str,
    commands: &mut UnboundedReceiver<DebugCommand>,
    events: &UnboundedSender<DebugEvent>,
) -> Result<(), String> {
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|e| tr!("debug-listen-failed", port = port, error = e.to_string()))?;
    let _ = events.send(DebugEvent::Output(tr!("debug-listening", port = port)));
    loop {
        let stream = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(e) => return Err(e.to_string()),
            },
            command = commands.recv() => match command {
                Some(DebugCommand::Stop) | None => return Ok(()),
                Some(DebugCommand::AddBreakpoint(file, line)) => {
                    breakpoints.push((file, line));
                    continue;
                }
                Some(DebugCommand::RemoveBreakpoint(file, line)) => {
                    breakpoints.retain(|b| *b != (file.clone(), line));
                    continue;
                }
                Some(_) => continue,
            },
        };
        let mut connection = Connection::new(stream);
        let served = drive(
            &mut connection,
            breakpoints.clone(),
            commands,
            events,
            None,
            map,
            first,
        )
        .await;
        let _ = connection.call("detach", "", None).await;
        let _ = events.send(DebugEvent::Output(tr!("debug-request-done")));
        if served.is_err() {
            continue;
        }
        let _ = events.send(DebugEvent::Running);
    }
}

/// Serves one Xdebug connection: reads `init`, configures features and
/// breakpoints, runs, and then answers UI commands until the script ends
/// or the UI stops it.
///
/// # Errors
/// Returns socket or protocol errors.
async fn drive(
    connection: &mut Connection,
    initial_breakpoints: Vec<(PathBuf, usize)>,
    commands: &mut UnboundedReceiver<DebugCommand>,
    events: &UnboundedSender<DebugEvent>,
    mut child: Option<&mut Child>,
    map: &PathMap<'_>,
    first: &'static str,
) -> Result<(), String> {
    connection.read_packet().await?; // the `init` packet
    let _ = events.send(DebugEvent::Connected);
    for (name, value) in [
        ("max_depth", "1"),
        ("max_children", "100"),
        ("max_data", "4096"),
    ] {
        connection
            .call("feature_set", &format!("-n {name} -v {value}"), None)
            .await?;
    }
    let mut breakpoints = HashMap::new();
    for (file, line) in initial_breakpoints {
        connection
            .add_breakpoint(&mut breakpoints, file, line, map)
            .await?;
    }
    let mut pending: Vec<DebugCommand> = Vec::new();
    let mut next = Some(first);
    // One-shot breakpoint of a run to cursor, cleared at the next pause.
    let mut run_to: Option<String> = None;
    loop {
        if let Some(step) = next.take() {
            let _ = events.send(DebugEvent::Running);
            match connection
                .continue_with(step, commands, &mut pending, child.as_deref_mut())
                .await?
            {
                Paused::Stopped => break,
                Paused::At => {
                    if let Some(id) = run_to.take() {
                        // Already gone when it was the line that paused.
                        connection
                            .call("breakpoint_remove", &format!("-d {id}"), None)
                            .await?;
                    }
                    let mut frames = parse_stack(&connection.call("stack_get", "", None).await?);
                    for frame in &mut frames {
                        frame.file = map.local(&frame.file);
                    }
                    let _ = events.send(DebugEvent::Paused(frames));
                    let context = connection.call("context_get", "-d 0", None).await?;
                    let _ = events.send(DebugEvent::Variables(0, parse_context(&context)));
                }
            }
        }
        let command = match pending.pop() {
            Some(c) => Some(c),
            None => commands.recv().await,
        };
        let Some(command) = command else {
            break;
        };
        match command {
            DebugCommand::Resume => next = Some("run"),
            DebugCommand::StepOver => next = Some("step_over"),
            DebugCommand::StepInto => next = Some("step_into"),
            DebugCommand::StepOut => next = Some("step_out"),
            DebugCommand::Stop => break,
            DebugCommand::SelectFrame(depth) => {
                let context = connection
                    .call("context_get", &format!("-d {depth}"), None)
                    .await?;
                let _ = events.send(DebugEvent::Variables(depth, parse_context(&context)));
            }
            DebugCommand::Expand { fullname, depth } => {
                let escaped = fullname.replace('\\', "\\\\").replace('"', "\\\"");
                let response = connection
                    .call(
                        "property_get",
                        &format!("-d {depth} -n \"{escaped}\""),
                        None,
                    )
                    .await?;
                let members = response
                    .children_named("property")
                    .next()
                    .map(|p| parse_property(p).children)
                    .unwrap_or_default();
                let _ = events.send(DebugEvent::Children(fullname, members));
            }
            DebugCommand::AddBreakpoint(file, line) => {
                connection
                    .add_breakpoint(&mut breakpoints, file, line, map)
                    .await?;
            }
            DebugCommand::RemoveBreakpoint(file, line) => {
                if let Some(id) = breakpoints.remove(&(file, line)) {
                    connection
                        .call("breakpoint_remove", &format!("-d {id}"), None)
                        .await?;
                }
            }
            DebugCommand::RunTo(file, line) => {
                let args = format!(
                    "-t line -f {} -n {} -r 1",
                    file_uri(&map.remote(&file)),
                    line + 1
                );
                let response = connection.call("breakpoint_set", &args, None).await?;
                run_to = response.attr("id").map(str::to_string);
                next = Some("run");
            }
            DebugCommand::Eval(expression) => {
                let response = connection.call("eval", "", Some(&expression)).await?;
                let result = match response_error(&response) {
                    Some(error) => Err(error),
                    None => response
                        .children_named("property")
                        .next()
                        .map(parse_property)
                        .ok_or_else(|| tr!("debug-no-value")),
                };
                let _ = events.send(DebugEvent::Evaluated(expression, result));
            }
        }
    }
    Ok(())
}

/// Where a continuation command left the script.
enum Paused {
    /// Stopped at a line.
    At,
    /// The script ended (or was stopped).
    Stopped,
}

/// The TCP connection to Xdebug.
struct Connection {
    /// Socket Xdebug connected on.
    stream: TcpStream,
    /// Packet framing state.
    reader: PacketReader,
    /// Next transaction id.
    transaction: u32,
}

impl Connection {
    /// Wraps an accepted connection.
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            reader: PacketReader::default(),
            transaction: 1,
        }
    }

    /// Reads the next packet as an XML tree.
    ///
    /// # Errors
    /// Returns a message when the connection closes or sends bad XML.
    async fn read_packet(&mut self) -> Result<Element, String> {
        loop {
            if let Some(xml) = self.reader.next_packet() {
                return parse_xml(&xml);
            }
            let mut buf = [0u8; 8192];
            let n = self
                .stream
                .read(&mut buf)
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err(tr!("debug-connection-closed"));
            }
            self.reader.push(&buf[..n]);
        }
    }

    /// Sends a command without waiting for its response.
    ///
    /// # Errors
    /// Returns the socket error.
    async fn send(&mut self, name: &str, args: &str, data: Option<&str>) -> Result<(), String> {
        let bytes = encode_command(name, self.transaction, args, data);
        self.transaction += 1;
        self.stream
            .write_all(&bytes)
            .await
            .map_err(|e| e.to_string())
    }

    /// Sends a command and returns its response (skipping unrelated
    /// packets such as notifications).
    ///
    /// # Errors
    /// Returns socket or protocol errors.
    async fn call(
        &mut self,
        name: &str,
        args: &str,
        data: Option<&str>,
    ) -> Result<Element, String> {
        let id = self.transaction.to_string();
        self.send(name, args, data).await?;
        loop {
            let packet = self.read_packet().await?;
            if packet.name == "response" && packet.attr("transaction_id") == Some(id.as_str()) {
                return Ok(packet);
            }
        }
    }

    /// Sets a line breakpoint and remembers its id.
    ///
    /// # Errors
    /// Returns socket or protocol errors.
    async fn add_breakpoint(
        &mut self,
        ids: &mut HashMap<(PathBuf, usize), String>,
        file: PathBuf,
        line: usize,
        map: &PathMap<'_>,
    ) -> Result<(), String> {
        let args = format!(
            "-t line -f {} -n {}",
            file_uri(&map.remote(&file)),
            line + 1
        );
        let response = self.call("breakpoint_set", &args, None).await?;
        if let Some(id) = response.attr("id") {
            ids.insert((file, line), id.to_string());
        }
        Ok(())
    }

    /// Sends `run` or a step and waits for the script to stop again. A
    /// Stop from the UI meanwhile kills the process; other commands wait.
    ///
    /// # Errors
    /// Returns socket or protocol errors.
    async fn continue_with(
        &mut self,
        step: &str,
        commands: &mut UnboundedReceiver<DebugCommand>,
        pending: &mut Vec<DebugCommand>,
        child: Option<&mut Child>,
    ) -> Result<Paused, String> {
        let id = self.transaction.to_string();
        self.send(step, "", None).await?;
        loop {
            tokio::select! {
                packet = self.read_packet() => {
                    let packet = match packet {
                        Ok(p) => p,
                        // Xdebug closes the socket when the script ends.
                        Err(_) => return Ok(Paused::Stopped),
                    };
                    if packet.name != "response" || packet.attr("transaction_id") != Some(id.as_str()) {
                        continue;
                    }
                    return Ok(match packet.attr("status") {
                        Some("break") => Paused::At,
                        _ => Paused::Stopped,
                    });
                }
                command = commands.recv() => match command {
                    Some(DebugCommand::Stop) | None => {
                        if let Some(child) = child {
                            let _ = child.kill().await;
                        }
                        return Ok(Paused::Stopped);
                    }
                    Some(DebugCommand::AddBreakpoint(..) | DebugCommand::RemoveBreakpoint(..)) => {
                        if let Some(c) = command {
                            pending.insert(0, c);
                        }
                    }
                    // Stepping or inspecting only makes sense while paused.
                    Some(_) => {}
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::process::Command;

    /// Writes one DBGp packet as Xdebug frames it.
    async fn reply(stream: &mut TcpStream, xml: &str) {
        let packet = format!("{}\0{xml}\0", xml.len());
        stream.write_all(packet.as_bytes()).await.unwrap();
    }

    /// Plays Xdebug: answers each command with a canned response, breaking
    /// on the first `run` and ending on the second.
    async fn fake_xdebug(port: u16) -> Vec<String> {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        reply(
            &mut stream,
            r#"<init fileuri="file:///app/vendor/bin/pest" language="PHP"/>"#,
        )
        .await;
        let mut seen = Vec::new();
        let mut runs = 0;
        let mut buffer = Vec::new();
        loop {
            let mut chunk = [0u8; 4096];
            let n = stream.read(&mut chunk).await.unwrap_or(0);
            if n == 0 {
                return seen;
            }
            buffer.extend_from_slice(&chunk[..n]);
            while let Some(nul) = buffer.iter().position(|b| *b == 0) {
                let line = String::from_utf8_lossy(&buffer[..nul]).into_owned();
                buffer.drain(..=nul);
                let mut words = line.split_whitespace();
                let name = words.next().unwrap_or_default().to_string();
                let tx = words.nth(1).unwrap_or_default().to_string();
                seen.push(line.clone());
                let body = match name.as_str() {
                    "breakpoint_set" => r#" id="77""#.to_string(),
                    "run" => {
                        runs += 1;
                        if runs == 1 {
                            r#" status="break" reason="ok""#.to_string()
                        } else {
                            r#" status="stopping" reason="ok""#.to_string()
                        }
                    }
                    _ => String::new(),
                };
                let inner = match name.as_str() {
                    "stack_get" => {
                        r#"<stack where="{closure}" level="0" type="file" filename="file:///app/tests/UserTest.php" lineno="6"/>"#
                    }
                    "context_get" => {
                        r#"<property name="$x" fullname="$x" type="int"><![CDATA[5]]></property>"#
                    }
                    "eval" => r#"<property type="int"><![CDATA[6]]></property>"#,
                    "property_get" => {
                        r#"<property name="$u" fullname="$u" type="object" classname="User" children="1"><property name="id" fullname="$u-&gt;id" type="int"><![CDATA[1]]></property></property>"#
                    }
                    _ => "",
                };
                let xml = format!(
                    r#"<response command="{name}" transaction_id="{tx}"{body}>{inner}</response>"#
                );
                reply(&mut stream, &xml).await;
                if name == "detach" {
                    return seen;
                }
            }
        }
    }

    #[tokio::test]
    async fn drive_should_break_inspect_evaluate_and_finish() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let xdebug = tokio::spawn(fake_xdebug(port));
        let (stream, _) = listener.accept().await.unwrap();
        let mut connection = Connection::new(stream);
        let (command_tx, mut commands) = unbounded_channel();
        let (events_tx, mut events) = unbounded_channel();
        let mut child = Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        // The UI's side: react to events like the panel would.
        let ui = tokio::spawn(async move {
            let mut log = Vec::new();
            while let Some(event) = events.recv().await {
                match &event {
                    DebugEvent::Variables(..) => {
                        command_tx
                            .send(DebugCommand::Eval("$x + 1".into()))
                            .unwrap();
                        command_tx
                            .send(DebugCommand::Expand {
                                fullname: "$u".into(),
                                depth: 0,
                            })
                            .unwrap();
                    }
                    DebugEvent::Children(..) => command_tx.send(DebugCommand::Resume).unwrap(),
                    _ => {}
                }
                log.push(event);
            }
            log
        });
        let breakpoints = vec![(PathBuf::from("/app/tests/UserTest.php"), 5)];
        let interpreter = Interpreter::default();
        let map = PathMap {
            root: Path::new("/app"),
            interpreter: &interpreter,
        };
        drive(
            &mut connection,
            breakpoints,
            &mut commands,
            &events_tx,
            Some(&mut child),
            &map,
            "run",
        )
        .await
        .unwrap();
        let _ = connection.call("detach", "", None).await;
        drop(events_tx);
        let log = ui.await.unwrap();
        let seen = xdebug.await.unwrap();

        assert!(seen.iter().any(|c| c.starts_with("breakpoint_set")
            && c.ends_with("-f file:///app/tests/UserTest.php -n 6")));
        assert!(
            log.iter()
                .any(|e| matches!(e, DebugEvent::Paused(f) if f[0].line == 5))
        );
        assert!(
            log.iter()
                .any(|e| matches!(e, DebugEvent::Variables(0, v) if v[0].value == "5"))
        );
        assert!(log.iter().any(
            |e| matches!(e, DebugEvent::Evaluated(x, Ok(v)) if x == "$x + 1" && v.value == "6")
        ));
        assert!(log.iter().any(
            |e| matches!(e, DebugEvent::Children(n, m) if n == "$u" && m[0].fullname == "$u->id")
        ));
        assert_eq!(seen.iter().filter(|c| c.starts_with("run")).count(), 2);
        let _ = child.kill().await;
    }
}
