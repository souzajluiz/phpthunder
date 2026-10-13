//! Debugger data: frames, variables, commands to the session and events
//! from it.

use std::path::PathBuf;

/// One frame of the call stack, innermost first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Depth (0 is where execution stopped); `context_get -d` takes it.
    pub level: usize,
    /// Function or method name (`{closure}`, `App\User->save`).
    pub function: String,
    /// File of the frame.
    pub file: PathBuf,
    /// 0-based line.
    pub line: usize,
}

/// A variable or a member of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Display name (`$user`, `name`, `0`).
    pub name: String,
    /// Expression that reaches it (`$user->name`), for expanding.
    pub fullname: String,
    /// Type, or the class for objects.
    pub kind: String,
    /// Scalar value (decoded); empty for arrays and objects.
    pub value: String,
    /// Whether it has members to expand.
    pub has_children: bool,
    /// Members Xdebug sent along (up to the configured depth).
    pub children: Vec<Variable>,
}

/// What the UI asks the debug session to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugCommand {
    /// Continue to the next breakpoint.
    Resume,
    /// Run to the next line in this function.
    StepOver,
    /// Step into the call on this line.
    StepInto,
    /// Run until the current function returns.
    StepOut,
    /// Stop the script.
    Stop,
    /// Show the variables of a stack frame.
    SelectFrame(usize),
    /// Load the members of a variable in a frame.
    Expand {
        /// Expression of the variable.
        fullname: String,
        /// Stack depth it belongs to.
        depth: usize,
    },
    /// Add a line breakpoint (0-based line).
    AddBreakpoint(PathBuf, usize),
    /// Remove a line breakpoint (0-based line).
    RemoveBreakpoint(PathBuf, usize),
    /// Evaluate a PHP expression where execution is paused (Xdebug
    /// evaluates in the innermost frame).
    Eval(String),
}

/// What the debug session reports to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugEvent {
    /// A line the test process printed.
    Output(String),
    /// Xdebug connected; the script runs to the first breakpoint.
    Connected,
    /// The script runs (after resume or a step).
    Running,
    /// Execution stopped at a breakpoint or after a step.
    Paused(Vec<Frame>),
    /// Variables of the frame at a depth.
    Variables(usize, Vec<Variable>),
    /// Members of a variable, by its expression.
    Children(String, Vec<Variable>),
    /// Result of an evaluation: the expression and its value or error.
    Evaluated(String, Result<Variable, String>),
    /// The session ended; carries an error when it failed.
    Finished(Option<String>),
}
