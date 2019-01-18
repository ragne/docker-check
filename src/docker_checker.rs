use dockworker::{container::Container, container::ContainerFilters, Docker};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct ContainerStats {
   pub count: u32,
   pub restarts: u32,
   pub sequent_failures: u16,
}
pub type Stats = HashMap<String, ContainerStats>;


pub struct DockerChecker {
    is_finished: Arc<AtomicBool>,
    client: Docker,
    stats: Stats}

impl DockerChecker {
    pub fn new(connect_str: &str, finished: Arc<AtomicBool>) -> Result<Self, String> {
        let client;
        if connect_str.starts_with("http") {
            client = Docker::connect_with_http(connect_str).map_err(|e| e.to_string())?;
        } else if connect_str.starts_with("unix") {
            client = Docker::connect_with_unix(connect_str).map_err(|e| e.to_string())?;
        } else {
            return Err(format!(
                "Connection to URI: {} cannot be established (protocol may be unsupported yet)",
                connect_str
            ));
        };
        Ok(Self {
            client,
            is_finished: finished,
            stats: HashMap::new()
        })
    }

    pub fn watch_for(
        &mut self,
        sleep_for: Duration,
        callback: fn(&Docker, &Container, &mut self::Stats) -> (),
    ) -> Result<(), String> {
        while !self.is_finished.load(Ordering::Relaxed) {
            let filter = ContainerFilters::new();
            let containers = self
                .client
                .list_containers(None, None, None, filter)
                .unwrap();
            containers.iter().for_each(|c| {
                debug!("Got container {:?}: calling callback", c);
                callback(&self.client, &c, &mut self.stats);
            });
            thread::sleep(sleep_for);
        }
        Ok(())
    }
}
