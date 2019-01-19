use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
mod docker_checker;
mod server_blocking;
extern crate config as configuration;
extern crate ctrlc;

extern crate regex;
extern crate serde;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;

mod threadpool;
extern crate dockworker;
pub mod config;

use config::LoggingConfig;
use dockworker::{container::ContainerFilters, Docker};
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

fn handle_client(mut stream: TcpStream, finished: &Arc<AtomicBool>) {
    // ...
    let mut should_stop = false;
    let response = format!(
        "HTTP/1.1 200 OK{ending}\
         Content-Type: text/plain{ending}\
         Content-Length: 0{ending}{ending}",
        ending = "\r\n"
    );
    trace!("in handle_client!");
    loop {
        let mut buf = vec![0; 512];
        match stream.read(&mut buf) {
            Ok(read_bytes) => {
                if read_bytes == 0 || read_bytes < buf.len() {
                    should_stop = true
                }
                trace!("read {} bytes!", read_bytes);
                trace!(
                    "trying to decode: {:?}\n",
                    String::from_utf8_lossy(&buf[0..read_bytes])
                );
                if String::from_utf8_lossy(&buf[0..read_bytes]).contains("stop") {
                    finished.store(true, Ordering::SeqCst);
                }
                match stream.write(response.as_bytes()) {
                    Ok(bytes_written) => debug!("wrote {} bytes!", bytes_written),
                    Err(ref e) => match e.kind() {
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe => {
                            should_stop = true
                        }
                        x => {
                            warn!("stream.read::Error: {:?}", x);
                            should_stop = true
                        }
                    },
                };
                stream.flush().unwrap();
            }
            Err(ref e) => match e.kind() {
                std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe => {
                    should_stop = true
                }
                x => {
                    warn!("stream.read::Error: {:?}", x);
                    should_stop = true
                }
            },
        };

        if should_stop {
            debug!("EOF reached or stop was signalled explicitly!");
            stream.shutdown(Shutdown::Both).unwrap_or(());
            break;
        }
    }
}

use docker_checker::{ContainerStats, Stats};
use dockworker::container::HealthState;

fn listen(finished: Arc<AtomicBool>) -> Result<(), String> {
    let mut dc =
        docker_checker::DockerChecker::new(&SETTINGS.docker.connect_uri, finished, 
        &*SETTINGS)?;
    dc.watch_for(
        Duration::from_secs(2),
        |client: &Docker, container: &dockworker::container::Container, stats: &mut Stats| {
            let info = client.container_info(container)
                       .map_err(|e| error!("Error getting info: {}", e)).unwrap();
            let container_state = match info.State.Health{
                Some(health_state) => health_state.Status,
                None => { warn!("Container {} doesn't have a healthcheck, skipping..", &info.Name); return }
            };
            let container_stats = stats.entry(info.Name.clone()).or_insert(ContainerStats::default());
            if container_state == HealthState::Healthy {
                debug!("Container {} is okay: {:?}", &info.Name, container_stats);
                container_stats.count += 1;
            } else if container_state == HealthState::Unhealthy {
                debug!("Container {} is not okay, restarting; After 5 failures it will be restarted! Current count: {}", &info.Name, container_stats.consecutive_failures);
                container_stats.consecutive_failures += 1;
                if container_stats.consecutive_failures == SETTINGS.containers.consecutive_failures {
                client
                    .restart_container(&container.Id, Duration::from_secs(5))
                    .unwrap();
                    container_stats.restarts += 1;
                    container_stats.consecutive_failures = 0;
                }
            } else {
                debug!("Container is in state: {}", container_state);
            }
        },
    )
    .map_err(|e| error!("Error getting info: {}", e)).unwrap();

    Ok(())
}

fn main() {
    let settings = &SETTINGS;
    println!("Settings: {:?}", **settings);
    setup_logger(&settings.logging).unwrap();

    // no networking for now
    // let mut s = server_blocking::Server::new("127.0.0.1:8000", 0);
    let finished = Arc::new(AtomicBool::new(false));
    let f2 = finished.clone();
    ctrlc::set_handler(move || {
        f2.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
    // thread::spawn(move || {
    //     s.serve(handle_client);
    // });

    /* 
        TODO: the following
            ! Log to the syslog
            ! Add rusoto and send-healthcheck-request
            ! Use `hard_failures` from config
            ! Ability to run a script if `hard_failures` from config is reached
    */

    listen(finished.clone())
        .map_err(|e| error!("Fatal: {}", e))
        .unwrap_or(());
    info!("Done");
    info!("Stopping~!");
    finished.store(true, Ordering::SeqCst);
}
