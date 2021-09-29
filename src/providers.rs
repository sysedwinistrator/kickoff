use std::collections::HashMap;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tokio::task::JoinHandle;
use std::fmt::Display;
use std::fmt;
use strum_macros;

use crate::history;
use crate::config::Config;

#[derive(strum_macros::ToString, Clone)]
pub enum ProviderType {
    PATH,
}

#[derive(Clone)]
pub struct ProviderItem {
    pub name: String,
    pub provider_type: ProviderType,
    pub executable: Option<String>,
    pub base_score: usize
}

impl Default for ProviderItem {
    fn default() -> Self {
        ProviderItem {
            name: "".to_owned(),
            provider_type: ProviderType::PATH,
            executable: None,
            base_score: 1
        }
    }
}

impl Display for ProviderItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub trait Provider {
    fn get_options(&self) -> Option<JoinHandle<Vec<ProviderItem>>>;
}

pub struct PathProvider {
    history: HashMap<String, usize>,
}

impl PathProvider {
    pub fn new(config: Config) -> Self {
        let decrease_interval = config.history.decrease_interval;
        PathProvider {
            history: history::get_history(decrease_interval, ProviderType::PATH.to_string()).unwrap_or(HashMap::new()),
        }
    }
}

impl Provider for PathProvider {
    fn get_options(&self) -> Option<JoinHandle<Vec<ProviderItem>>> {
        let var = match env::var_os("PATH") {
            Some(var) => var,
            None => return None,
        };

        let history = self.history.clone();
        Some(tokio::spawn(async move {
            let mut res: Vec<ProviderItem> = Vec::new();

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
                    let executable = exe.file_name().to_str().unwrap().to_string();
                    res.push(ProviderItem {
                        name: executable.clone(),
                        provider_type: ProviderType::PATH,
                        executable: Some(executable.clone()),
                        base_score: history.get(&executable).unwrap_or(&0).clone(),
                    });
                }
            }

            res
        }))
    }
}