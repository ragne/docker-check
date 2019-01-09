use std::io::prelude::*;
use std::io::ErrorKind;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc};
use std::thread;
use std::time::Duration;

use super::threadpool;

pub struct Server {
    listener: TcpListener,
    threadpool: Option<threadpool::ThreadPool>,
    finished: Arc<AtomicBool>,
}

impl Server {
    pub fn new(addr: &str, num_threads: usize) -> Self {
        let mut threadpool = None;
        let listener = TcpListener::bind(addr).expect("Failed to bind socket");
        listener
            .set_nonblocking(true)
            .expect("Failed to enter non-blocking mode");

        // Poll for data every 5 milliseconds for 5 seconds.
        let finished = Arc::new(AtomicBool::new(false));
        if num_threads > 0 {
            threadpool = Some(threadpool::ThreadPool::new(num_threads));
        }
        Self {
            listener,
            finished,
            threadpool,
        }
    }

    pub fn get_finished(&self) -> Arc<AtomicBool> {
        self.finished.clone()
    }

    pub fn serve(&mut self, f: fn(TcpStream, &Arc<AtomicBool>) -> ()) {
        let finished_clone = self.finished.clone();
        let use_pool = self.threadpool.is_some();

        while !finished_clone.load(Ordering::Relaxed) {
            match self.listener.accept() {
                Ok((_socket, addr)) => {
                    warn!("new client: {:?}", addr);
                    if use_pool {
                        let fin = self.finished.clone();
                        match &self.threadpool {
                            Some(pool) => pool.execute(move || f(_socket, &fin)),
                            None => {}
                        }
                    } else {
                        f(_socket, &finished_clone);
                    }
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // wait until network socket is ready, typically implemented
                    // via platform-specific APIs such as epoll or IOCP
                    thread::sleep(Duration::from_millis(250));
                    continue;
                }
                Err(e) => panic!("encountered IO error: {}", e),
            }
        }
    }
}
