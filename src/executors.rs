use crate::config::Config;
use crate::history;
use nix::unistd::{fork, ForkResult};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::{env, fs};

trait Executor {
    fn new(config: Config) -> Self;
    fn get_options() -> Option<Vec<String>>;
    fn run(&mut self, query: String);
    fn get_scores(&mut self) -> Option<HashMap<String, usize>>;
}

struct Application {
    history: Option<HashMap<String, usize>>,
    config: Config,
}

impl Executor for Application {
    fn new(config: Config) -> Self {
        Application {
            history: None,
            config: config.clone(),
        }
    }

    fn get_options() -> Option<Vec<String>> {
        let var = match env::var_os("PATH") {
            Some(var) => var,
            None => return None,
        };

        let mut res: Vec<String> = Vec::new();

        let paths_iter = env::split_paths(&var);
        let dirs_iter = paths_iter.filter_map(|path| fs::read_dir(path).ok());

        for dir in dirs_iter {
            let executables_iter = dir.filter_map(|file| file.ok()).filter(|file| {
                if let Ok(metadata) = file.metadata() {
                    return !metadata.is_dir() && metadata.permissions().mode() & 0o111 != 0;
                }
                false
            });

            for exe in executables_iter {
                res.push(exe.file_name().to_str().unwrap().to_string());
            }
        }

        Some(res)
    }

    fn get_scores(&mut self) -> Option<HashMap<String, usize>> {
        history::get_history(self.config.history.decrease_interval)
    }

    fn run(&mut self, query: String) {
        if let Ok(mut args) = shellwords::split(&query) {
            match unsafe { fork() } {
                Ok(ForkResult::Parent { .. }) => {
                    let mut history = self.get_scores().unwrap();
                    history.insert(query.to_string(), history.get(&query).unwrap_or(&0) + 1);
                    match history::commit_history(&history) {
                        Ok(_) => {}
                        Err(e) => {
                            println!("{}", e.to_string())
                        }
                    };
                }
                Ok(ForkResult::Child) => {
                    let err = exec::Command::new(args.remove(0)).args(&args).exec();
                    panic!("Error: {}", err);
                }
                Err(_) => {
                    panic!("failed to fork");
                }
            }
        }
    }
}
