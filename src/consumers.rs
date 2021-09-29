use crate::history;
use crate::providers::ProviderItem;
use log::error;
use nix::{
    sys::wait::{waitpid, WaitPidFlag, WaitStatus},
    unistd::{fork, ForkResult},
};
use notify_rust::Notification;
use std::collections::HashMap;
use std::error::Error;
use std::process;
use std::time::Duration;
use tokio::task::JoinHandle;

pub trait Consumer {
    fn run(&mut self, query: &ProviderItem) -> Result<Option<JoinHandle<()>>, Box<dyn Error>>;
}

pub struct StdoutConsumer {}

impl StdoutConsumer {
    pub fn new() -> Self {
        StdoutConsumer {}
    }
}

impl Consumer for StdoutConsumer {
    fn run(&mut self, item: &ProviderItem) -> Result<Option<JoinHandle<()>>, Box<dyn Error>> {
        println!("{}", item);
        Ok(None)
    }
}

pub struct ExecConsumer {
    history: Option<HashMap<String, usize>>,
}

impl ExecConsumer {
    pub fn new(history: Option<HashMap<String, usize>>) -> Self {
        ExecConsumer { history }
    }
}

impl Consumer for ExecConsumer {
    fn run(&mut self, item: &ProviderItem) -> Result<Option<JoinHandle<()>>, Box<dyn Error>> {
        if let Some(history) = &self.history {
            if let Ok(mut args) = shellwords::split(&item.executable.expect("Item is not executable")) {
                match unsafe { fork() } {
                    Ok(ForkResult::Parent { child }) => {
                        let mut history = history.clone();
                        return Ok(Some(tokio::spawn(async move {
                            tokio::time::sleep(Duration::new(1, 0)).await;
                            match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
                                Ok(WaitStatus::StillAlive) | Ok(WaitStatus::Exited(_, 0)) => {
                                    history.insert(
                                        item.name,
                                        history.get(&query).unwrap_or(&0) + 1,
                                    );
                                    match history::commit_history(&history) {
                                        Ok(_) => {}
                                        Err(e) => {
                                            error!("{}", e.to_string())
                                        }
                                    };
                                }
                                Ok(_) => {
                                    /* Every non 0 statuscode holds no information since it's
                                    origin can be the started application or a file not found error.
                                    In either case the error has already been logged and does not
                                    need to be handled here. */
                                }
                                Err(err) => error!("{}", err),
                            }
                        })));
                    }
                    Ok(ForkResult::Child) => {
                        let err = exec::Command::new(args.remove(0)).args(&args).exec();

                        // Won't be executed when exec was successful
                        error!("{}", err);
                        Notification::new()
                            .summary("Kickoff")
                            .body(&format!("{}", err))
                            .timeout(5000)
                            .show()?;
                        process::exit(2);
                    }
                    Err(err) => Err(Box::new(err)),
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
}
