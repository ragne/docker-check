use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
mod docker_checker;
extern crate config as configuration;
extern crate ctrlc;

extern crate os_pipe;
extern crate regex;
extern crate serde;

#[macro_use]
extern crate human_panic;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;

extern crate dockworker;
pub mod config;
mod run_command;

use config::LoggingConfig;
use std::str::FromStr;

lazy_static! {
    static ref SETTINGS: config::Config = {
        config::get_settings()
            .map_err(|e| warn!("Cannot read config. Error: {}", e))
            .unwrap_or_default()
    };
}

pub fn setup_logger(config: &LoggingConfig) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::from_str(&config.default).unwrap_or(log::LevelFilter::Warn))
        .level_for(
            "docker_check",
            log::LevelFilter::from_str(&config.checker).unwrap_or(log::LevelFilter::Warn),
        )
        .chain(std::io::stdout())
        .chain(fern::log_file("output.log")?)
        .apply()?;
    Ok(())
}

use docker_checker::{ContainerStats, DockerChecker};
use dockworker::container::{Container, HealthState};

fn check_docker_containers(finished: Arc<AtomicBool>) -> Result<(), String> {
    let mut dc = DockerChecker::new(&SETTINGS.docker.connect_uri, finished, &*SETTINGS)?;
    dc.watch_for(Duration::from_secs(2), |this: &DockerChecker, container: &Container| {
        let info;
        let client = &this.client;
        let stats = &mut this.stats.borrow_mut();
        let config = &this.config;
        match client.container_info(container) {
            Ok(x) => {
                info = x;
            }
            Err(e) => {
                error!(
                    "Error getting info for container (could be possible that container was removed): {}. Skipping..",
                    e
                );
                return;
            }
        };
        let container_state = match info.State.Health {
            Some(health_state) => health_state.Status,
            None => {
                warn!("Container {} doesn't have a healthcheck, skipping..", &info.Name);
                return;
            }
        };
        let container_stats = stats.entry(info.Id.clone()).or_insert(ContainerStats::default());
        if container_state == HealthState::Healthy {
            debug!("Container {} is okay: {:?}", &info.Name, container_stats);
            container_stats.count += 1;
        } else if container_state == HealthState::Unhealthy {
            debug!(
                "Container {} is not okay, restarting; After {} failures it will be restarted! Current count: {}",
                &info.Name, config.containers.consecutive_failures, container_stats.consecutive_failures
            );
            container_stats.consecutive_failures += 1;

            if container_stats.consecutive_failures - 1 == config.containers.consecutive_failures {
                warn!(
                    "Container {} scored {} consecutive_failures and going to be restarted",
                    &info.Name, config.containers.consecutive_failures
                );
                let failed_container = container.Id.clone();
                thread::spawn(move || {
                    // Won't block the main thread anymore, rarely can fail.
                    let client_for_restart = DockerChecker::get_new_client(&SETTINGS.docker.connect_uri);
                    match client_for_restart {
                        Ok(client) => {
                            client
                                .restart_container(&failed_container, Duration::from_secs(5))
                                .unwrap();
                            warn!("Container {} restarted successfully!", &failed_container);
                        }
                        Err(e) => error!(
                            "Cannot get the docker client. URI: {}, error: {}",
                            &SETTINGS.docker.connect_uri, e
                        ),
                    }
                });

                container_stats.restarts += 1;
                container_stats.consecutive_failures = 0;

                if container_stats.restarts >= config.containers.hard_failures as u32 {
                    let mut args = Vec::new();
                    args.push(container.Id.clone());
                    let cmd = config.containers.run_on_failure.clone();
                    thread::spawn(move || {
                        let result = run_command::run_command(&cmd, &args);
                        match result {
                            Ok(output) => {
                                if !output.status.success() {
                                    warn!(
                                        "Executed script \"{}\".Got non-zero({}) exit status.\nOutput: {}",
                                        cmd, output.status, output.output
                                    );
                                } else {
                                    debug!("Executed script \"{}\" successfully.\nOutput: {}", cmd, output.output);
                                }
                            }
                            Err(e) => {
                                warn!("Cannot execute command \"{}\". Error: {:?}", cmd, e);
                            }
                        }
                    });
                }
            }
        } else {
            debug!("Container {} is in state: {}", &info.Name, container_state);
        }
    })
    .map_err(|e| error!("Error getting info: {}", e))
    .unwrap();

    Ok(())
}

fn main() {
    // At least that will allow some reports (hope it'll never fire though)
    // But there are a bit of unwraps scattered over the place
    setup_panic!();
    let settings = &SETTINGS;
    debug!("Got settings: {:?}", **settings);
    setup_logger(&settings.logging).expect("Cannot setup logger. Shouldn't be possible in most cases");

    // no networking for now
    let finished = Arc::new(AtomicBool::new(false));
    let f2 = finished.clone();
    ctrlc::set_handler(move || {
        f2.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    /*
        TODO: the following
            ! Log to the syslog
            ! Add rusoto and send-healthcheck-request
    */

    check_docker_containers(finished.clone())
        .map_err(|e| error!("Fatal: {}", e))
        .unwrap_or(());
    info!("Done");
    info!("Stopping~!");
    finished.store(true, Ordering::SeqCst);
}
