use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
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

fn listen(finished: Arc<AtomicBool>) -> io::Result<()> {
    let docker = Docker::connect_with_http("http://localhost:2376").unwrap();

    while !finished.load(Ordering::Relaxed) {
        let filter = ContainerFilters::new();
        let containers = docker.list_containers(None, None, None, filter).unwrap();
        containers.iter().for_each(|c| {
            let res = docker.container_info(&c).unwrap();
            debug!("{:?}: {:?}", res.Name, res.State.Health.Status);
        });
        thread::sleep(Duration::from_secs(2));
    }
    Ok(())
}

fn main() {
    setup_logger().unwrap();

    let mut s = server_blocking::Server::new("127.0.0.1:8000", 0);
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
