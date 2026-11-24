//! Background solver execution for the TUI.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde_json::Value;

use crate::config::Config;
use crate::core::Runner;
use crate::result::{self, RunResult};
use crate::scheme::Method;

/// Event emitted by a background run.
#[derive(Debug)]
pub enum RunEvent {
    /// Progress message for the log pane.
    Log(String),
    /// Run finished and the output file was written.
    Done {
        /// Solver result.
        result: Box<RunResult>,
        /// Output file path.
        output: PathBuf,
    },
    /// Run failed or was cancelled.
    Failed(String),
}

/// Handle to a background run.
pub struct RunHandle {
    /// Receives events from the worker.
    pub receiver: Receiver<RunEvent>,
    /// Set to `true` to request cancellation.
    pub cancel: Arc<AtomicBool>,
}

/// Spawn one run in a worker thread.
pub fn spawn(
    name: String,
    configuration: Value,
    config: Config,
    scheme: Option<Method>,
    output: PathBuf,
    source: Option<String>,
) -> RunHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();
    let flag = Arc::clone(&cancel);

    thread::spawn(move || {
        let _ = sender.send(RunEvent::Log(format!("starting {name}")));
        let runner = match scheme {
            Some(method) => Runner::with_scheme(config, method),
            None => Runner::new(config),
        };
        let outcome = runner.map(|runner| runner.with_cancel(flag).run());

        match outcome {
            Ok(Ok(result_value)) => {
                let payload = result::payload(
                    vec![result::run_record(
                        &name,
                        &configuration,
                        &result_value,
                        source.as_deref(),
                    )],
                    vec![result_value.spatial_scheme.as_str().to_string()],
                    0,
                );
                match result::write_payload(&output, &payload) {
                    Ok(()) => {
                        let _ = sender.send(RunEvent::Done {
                            result: Box::new(result_value),
                            output,
                        });
                    }
                    Err(error) => {
                        let _ = sender.send(RunEvent::Failed(format!("write failed: {error}")));
                    }
                }
            }
            Ok(Err(error)) => {
                let _ = sender.send(RunEvent::Failed(error.to_string()));
            }
            Err(error) => {
                let _ = sender.send(RunEvent::Failed(error.to_string()));
            }
        }
    });

    RunHandle { receiver, cancel }
}
