use crate::{controller::FanController, logging::log};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::{sync::mpsc, time::Instant};
/// Blocking service runner; the controller determines retry deadlines.
pub fn run(mut controller: FanController, once: bool) -> Result<(), String> {
    let mut signals = Signals::new([SIGTERM, SIGINT]).map_err(|e| format!("signal setup: {e}"))?;
    let handle = signals.handle();
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            if let Some(signal) = signals.forever().next() {
                let _ = sender.send(signal);
            }
        })
        .map_err(|e| format!("signal thread: {e}"))?;
    let start = Instant::now();
    let result = loop {
        if receiver.try_recv().is_ok() {
            break controller.shutdown();
        }
        let step = controller.tick(start.elapsed());
        if once {
            break if receiver.try_recv().is_ok() {
                controller.shutdown()
            } else {
                step
            };
        }
        match receiver.recv_timeout(controller.wait_duration()) {
            Ok(signal) => {
                log("info", format!("received signal {signal}; shutting down"));
                break controller.shutdown();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("signal channel disconnected".into())
            }
        }
    };
    handle.close();
    worker
        .join()
        .map_err(|_| "signal thread failed".to_owned())?;
    result
}
