use smithay_client_toolkit::{
    default_environment,
    environment::SimpleGlobal,
    new_default_environment,
    reexports::{calloop, protocols::wlr::unstable::layer_shell::v1::client::zwlr_layer_shell_v1},
    WaylandSource,
};

use image::ImageBuffer;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;
use std::{cmp, env, process};

use crate::gui::{Action, DData, RenderEvent};
use log::error;
use simplelog::{ColorChoice, Config as LogConfig, LevelFilter, TermLogger, TerminalMode};
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tokio::task::JoinHandle;
use consumers::{StdoutConsumer, ExecConsumer, Consumer};

mod cli;
mod color;
mod config;
mod consumers;
mod font;
mod gui;
mod history;

default_environment!(Env,
    fields = [
        layer_shell: SimpleGlobal<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    ],
    singles = [
        zwlr_layer_shell_v1::ZwlrLayerShellV1 => layer_shell
    ],
);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if let Some(child_handle) = run().await? {
        /* wait for check if comand exec was successful
           and history has been written
        */
        child_handle.await?;
    }
    Ok(())
}

async fn run() -> Result<Option<JoinHandle<()>>, Box<dyn Error>> {
    let cli_matches = cli::get_matches();
    TermLogger::init(
        LevelFilter::Warn,
        LogConfig::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let config = match config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            error!("{}", e);
            process::exit(1);
        }
    };

    let applications_handle = { tokio::spawn(async move { get_executable_names() }) };
    let history_handle = {
        let config = config.clone();
        tokio::spawn(async move { history::get_history(config.history.decrease_interval) })
    };
    let font_handle = {
        let config = config.clone();
        tokio::spawn(async move { font::Font::new(&config.font, config.font_size) })
    };

    let (env, display, queue) =
        new_default_environment!(Env, fields = [layer_shell: SimpleGlobal::new(),])
            .expect("Initial roundtrip failed!");

    let history = history_handle.await?.unwrap_or_default();

    let mut consumer: Box<dyn Consumer> = match cli_matches.value_of("CONSUMER") {
        Some("stdout") => Box::new(StdoutConsumer::new()),
        Some("exec") => Box::new(ExecConsumer::new(Some(history.clone()))),
        Some(s) => {
            error!("Unknown consumer: {}", s);
            process::exit(1);
        }
        None => {
            error!("No consumer set");
            process::exit(1);
        }
    };

    let mut applications = applications_handle.await?.unwrap();
    for app in history.keys() {
        applications.push(app.to_string());
    }

    applications.sort();
    applications.dedup();
    applications.sort_by(|a, b| {
        history
            .get(b)
            .unwrap_or(&0)
            .cmp(history.get(a).unwrap_or(&0))
    });

    let layer_shell = env.require_global::<zwlr_layer_shell_v1::ZwlrLayerShellV1>();
    let pools = env
        .create_double_pool(|_| {})
        .expect("Failed to create a memory pool!");
    let surface = env.create_surface().detach();
    let mut surface = gui::Surface::new(None, surface, &layer_shell, pools);

    let mut event_loop = calloop::EventLoop::<DData>::try_new().unwrap();
    WaylandSource::new(queue)
        .quick_insert(event_loop.handle())
        .unwrap();

    gui::register_inputs(&env.get_all_seats(), &event_loop);

    let mut matched_exe: Vec<&String> = applications.iter().collect();
    let mut need_redraw = false;
    let mut data = DData::new(&display);
    let mut selection = 0;
    let mut select_query = false;

    let mut font = font_handle.await?;

    loop {
        let gui::DData { query, action, .. } = &mut data;
        match surface.next_render_event.take() {
            Some(RenderEvent::Closed) => break,
            Some(RenderEvent::Configure { width, height }) => {
                need_redraw = surface.set_dimensions(width, height);
            }
            None => {}
        }
        if let Some(action) = action.take() {
            match action {
                Action::NavUp => {
                    need_redraw = true;
                    if selection > 0 {
                        selection -= 1;
                    } else if !query.is_empty() {
                        select_query = true;
                    }
                }
                Action::NavDown => {
                    need_redraw = true;
                    if select_query && !matched_exe.is_empty() {
                        select_query = false;
                    } else if !matched_exe.is_empty() && selection < matched_exe.len() - 1 {
                        selection += 1;
                    }
                }
                Action::Search => {
                    need_redraw = true;
                    matched_exe = fuzzy_sort(&applications, query, &history);
                    select_query = false;
                    selection = 0;
                    if matched_exe.is_empty() {
                        select_query = true
                    }
                }
                Action::Complete => {
                    if !select_query {
                        let app = matched_exe.get(selection).unwrap();
                        if query == *app {
                            selection = if selection < matched_exe.len() - 1 {
                                selection + 1
                            } else {
                                selection
                            };
                        }
                        query.clear();
                        query.push_str(matched_exe.get(selection).unwrap());
                        need_redraw = true;
                    }
                }
                Action::Execute => {
                    let query = if select_query {
                        query.to_string()
                    } else {
                        matched_exe.get(selection).unwrap().to_string()
                    };
                    return consumer.run(query);
                }
                Action::Exit => break,
            }
        }

        if need_redraw {
            need_redraw = false;

            let mut img = ImageBuffer::from_pixel(
                surface.dimensions.0,
                surface.dimensions.1,
                config.colors.background.to_rgba(),
            );
            let prompt_width = if !config.prompt.is_empty() {
                let (width, _) = font.render(
                    &config.prompt,
                    &config.colors.prompt,
                    &mut img,
                    config.padding,
                    config.padding,
                );
                width
            } else {
                0
            };

            if !query.is_empty() {
                let color = if select_query {
                    &config.colors.text_selected
                } else {
                    &config.colors.text_query
                };
                font.render(
                    &query,
                    color,
                    &mut img,
                    config.padding + prompt_width,
                    config.padding,
                );
            }

            let spacer = (1.5 * config.font_size) as u32;
            let max_entries = ((surface.dimensions.1 - 2 * config.padding - spacer) as f32
                / (config.font_size * 1.2)) as usize;
            let offset = if selection > (max_entries / 2) {
                (selection - max_entries / 2) as usize
            } else {
                0
            };

            for (i, matched) in matched_exe
                .iter()
                .enumerate()
                .take(cmp::min(max_entries + offset, matched_exe.len()))
                .skip(offset)
            {
                let color = if i == selection && !select_query {
                    &config.colors.text_selected
                } else {
                    &config.colors.text
                };
                font.render(
                    &matched,
                    color,
                    &mut img,
                    config.padding,
                    (config.padding
                        + spacer
                        + (i - offset) as u32 * (config.font_size * 1.2) as u32)
                        as u32,
                );
            }

            match surface.draw(&img) {
                Ok(_) => {}
                Err(e) => {
                    error!("{}", e);
                    need_redraw = false;
                }
            };
        }

        display.flush().unwrap();
        event_loop.dispatch(None, &mut data).unwrap();
    }
    Ok(None)
}

fn get_executable_names() -> Option<Vec<String>> {
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

fn fuzzy_sort<'a>(
    executables: &'a [String],
    pattern: &str,
    pre_scored: &'a HashMap<String, usize>,
) -> Vec<&'a String> {
    let matcher = SkimMatcherV2::default();
    let mut executables = executables
        .iter()
        .map(|x| {
            (
                matcher
                    .fuzzy_match(&x, &pattern)
                    .map(|score| score + *pre_scored.get(x).unwrap_or(&1) as i64),
                x,
            )
        })
        .collect::<Vec<(Option<i64>, &String)>>();
    executables.sort_by(|a, b| b.0.unwrap_or(0).cmp(&a.0.unwrap_or(0)));
    executables
        .into_iter()
        .filter(|x| x.0.is_some())
        .into_iter()
        .map(|x| x.1)
        .collect()
}
