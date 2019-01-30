use super::config::Config;
use dockworker::{container::Container, container::ContainerFilters, Docker};
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Default, Debug)]
pub struct ContainerStats {
    // (4294967295 * 2) / 60 / 60 / 24 / 365
    // (u32.MAX * 2 second tick) / mins / hours / days / years = 272 years should be enough for everyone
    pub count: u32,
    pub restarts: u32,
    pub consecutive_failures: u16,
}

// * Possible optimization: make this hashmap transient (having a entries with activity time, and if they aren't active for $time then purge them from hashmap)
pub type Stats = Rc<RefCell<HashMap<String, ContainerStats>>>;

pub struct DockerChecker<'a> {
    is_finished: Arc<AtomicBool>,
    pub client: Docker,
    pub stats: Stats,
    pub config: &'a Config,
}

impl<'a> DockerChecker<'a> {
    pub fn new(connect_str: &str, finished: Arc<AtomicBool>, config: &'a Config) -> Result<Self, String> {
        let client = DockerChecker::get_new_client(connect_str)?;
        Ok(Self {
            client,
            is_finished: finished,
            stats: Rc::new(RefCell::new(HashMap::new())),
            config: &config,
        })
    }

    pub fn get_new_client(connect_str: &str) -> Result<Docker, String> {
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
        Ok(client)
    }

    pub fn watch_for(
        &mut self,
        sleep_for: Duration,
        callback: fn(&DockerChecker, &Container) -> (),
    ) -> Result<(), String> {
        let re = Regex::new(&self.config.containers.filter_by).unwrap();
        let apply_to = &self.config.containers.apply_filter_to;
        while !self.is_finished.load(Ordering::Relaxed) {
            let filter = ContainerFilters::new();
            let containers = self
                .client
                .list_containers(None, None, None, filter)
                .map_err(|e| error!("Error listing containers: {}", e))
                .unwrap_or(Vec::new());
            containers
                .iter()
                .filter(|&i| {
                    let mut result = false;
                    if apply_to.should_filter_names() {
                        result = i
                            .Names
                            .iter()
                            .filter(|&name| re.is_match(name))
                            .peekable()
                            .peek()
                            .is_some();
                    } else if apply_to.should_filter_images() {
                        result |= re.is_match(&i.Image);
                    }
                    result
                })
                .for_each(|c| {
                    trace!("Got container {:?}: calling callback", c);
                    callback(&self, &c);
                });
            thread::sleep(sleep_for);
        }
        Ok(())
    }
}
