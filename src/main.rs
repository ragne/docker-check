use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
mod docker_checker;
mod server_blocking;
extern crate ctrlc;

#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;

mod threadpool;
extern crate dockworker;

use dockworker::{container::ContainerFilters, Docker};

pub fn setup_logger() -> Result<(), fern::InitError> {
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
        .level(log::LevelFilter::Warn)
        .level_for("docker_check", log::LevelFilter::Trace)
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

use dockworker::container::HealthState;
static mut tries: i32 = 0;

fn listen(finished: Arc<AtomicBool>) -> io::Result<()> {
    let dc = docker_checker::DockerChecker::new("unix:///var/run/docker.sock", finished).unwrap();
    dc.watch_for(
        Duration::from_secs(2),
        |client: &Docker, container: &dockworker::container::Container| {
            let info = client.container_info(container).unwrap();
            let container_state = info.State.Health.unwrap().Status;
            if container_state == HealthState::Healthy {
                println!("Container is okay:");
                unsafe { tries = 0; }
            } else if container_state == HealthState::Unhealthy {
                println!("Container is not okay, restarting; Giving 5 seconds to stop");
                client
                    .restart_container(&container.Id, Duration::from_secs(5))
                    .unwrap();
                unsafe {tries += 1; }
            } else {
                println!("Container is in state: {}", container_state);
            }
        },
    )
    .unwrap();

    Ok(())
}

fn main() {
    setup_logger().unwrap();

    let mut s = server_blocking::Server::new("127.0.0.1:8000", 8);
    let f = s.get_finished();
    let f2 = s.get_finished();
    ctrlc::set_handler(move || {
        f2.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
    thread::spawn(move || {
        s.serve(handle_client);
    });

    listen(f.clone()).unwrap();
    info!("Done");
    info!("Stopping~!");
    f.store(true, Ordering::SeqCst);
}
