use super::config::Config;
use dockworker::{container::Container, container::ContainerFilters, Docker};
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Default, Debug)]
pub struct ContainerStats {
    // (4294967295 * 2) / 60 / 60 / 24 / 365
    // (u32.MAX * 2 second tick) / mins / hours / days / years = 272 years should be enough for everyone
    pub count: u32,
    pub restarts: u32,
    pub consecutive_failures: u16,
    pub not_seen_since: Option<Instant>,
}

// * Possible optimization: make this hashmap transient (having a entries with activity time, and if they aren't active for $time then purge them from hashmap)
pub type Stats<'a> = Rc<RefCell<HashMap<String, ContainerStats>>>;

pub struct DockerChecker<'a> {
    is_finished: Arc<AtomicBool>,
    pub client: Docker,
    pub stats: Stats<'a>,
    pub config: &'a Config,
    self_re: Regex,
    filter_by_re: Regex,
}

impl<'a> DockerChecker<'a> {
    pub fn new(connect_str: &str, finished: Arc<AtomicBool>, config: &'a Config) -> Result<Self, String> {
        let client = DockerChecker::get_new_client(connect_str)?;
        let re = Regex::new(&config.containers.filter_by).map_err(|e| e.to_string())?;
        let self_re = Regex::new(&config.containers.filter_self).map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            is_finished: finished,
            stats: Rc::new(RefCell::new(HashMap::new())),
            config: &config,
            self_re,
            filter_by_re: re,
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

    pub(super) fn filter_containers(&self, i: &Container) -> bool {
        let apply_to = &self.config.containers.apply_filter_to;
        let re = &self.filter_by_re;
        let self_re = &self.self_re;
        // filter containers by predicates from config
        // should return true _if container should be passed to the callback_!
        // false means container is filtered
        let mut result = false;
        if apply_to.should_filter_labels() {
            result |= match i.Labels {
                Some(ref map) => map
                    .iter()
                    .filter(|(ref name, ref value)| {
                        ((re.is_match(name) || self_re.is_match(name))
                            || (re.is_match(value) || self_re.is_match(value)))
                    })
                    .peekable()
                    .peek()
                    .is_some(),
                None => false,
            }
        }
        if result {
            // label filter returned match - bail out
            return !result;
        }

        if apply_to.should_filter_names() {
            result |= i
                .Names
                .iter()
                .filter(|&name| re.is_match(name) || self_re.is_match(name))
                .peekable()
                .peek()
                .is_some();
        } else if apply_to.should_filter_images() {
            result |= re.is_match(&i.Image) || self_re.is_match(&i.Image);
        }
        result
    }

    pub(super) fn retain_old_containers(
        &self,
        active_containers: &mut Vec<String>,
        k: &String,
        v: &mut ContainerStats,
    ) -> bool {
        let result = active_containers.contains(k);
        if !result {
            let not_seen_for = v.not_seen_since.get_or_insert(Instant::now());
            if Instant::now().duration_since(*not_seen_for) >= Duration::from_secs(self.config.docker.purge_unseen) {
                warn!(
                    "Retain container {} from because it hasn't been active for at least {} seconds!",
                    k, self.config.docker.purge_unseen
                );
            } else {
                return !result;
            }
        }
        result
    }

    pub fn watch_for(
        &mut self,
        sleep_for: Duration,
        callback: fn(&DockerChecker, &Container) -> (),
    ) -> Result<(), String> {
        let mut active_containers: Vec<String> = Vec::new();
        while !self.is_finished.load(Ordering::Relaxed) {
            active_containers.clear();
            let filter = ContainerFilters::new();
            let containers = self
                .client
                .list_containers(None, None, None, filter)
                .map_err(|e| error!("Error listing containers: {}", e))
                .unwrap_or(Vec::new());
            containers.iter().filter(|&i| self.filter_containers(i)).for_each(|c| {
                active_containers.push(c.Id.clone());
                trace!("Got container {:?}: calling callback", c);
                callback(&self, &c);
            });
            self.stats
                .borrow_mut()
                .retain(|k, v| self.retain_old_containers(&mut active_containers, k, v));
            thread::sleep(sleep_for);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config;
    use dockworker::container::{Container, HostConfig, Port};

    fn create_mock_container(
        name: Option<String>,
        image: Option<String>,
        labels: Option<HashMap<String, String>>,
    ) -> Container {
        Container {
            Id: "dfdb8ee577c1".to_string(),
            Image: image.unwrap_or("ce94baa47eed".to_string()),
            Status: "running".to_string(),
            Command: "cmd".to_string(),
            Created: 1549220249,
            Names: vec![name.unwrap_or("something_useful".to_string()); 1],
            Ports: Vec::<Port>::new(),
            SizeRw: Some(42), // I guess it is optional on Mac.
            SizeRootFs: Some(67),
            Labels: labels,
            HostConfig: HostConfig {
                NetworkMode: "bridge".to_string(),
            },
        }
    }

    #[test]
    fn filter_containers_test() {
        let settings = config::get_settings("tests/settings").unwrap();
        let finished = Arc::new(AtomicBool::new(false));
        let dc = DockerChecker::new(&settings.docker.connect_uri, finished, &settings).unwrap();

        // filter by name
        assert!(
            dc.filter_containers(&create_mock_container(None, Some("filter_me".to_string()), None)),
            "Should not be filtered!"
        );

        // filter by image
        assert!(
            dc.filter_containers(&create_mock_container(Some("Random-image-id".to_string()), None, None)),
            "Should not be filtered!"
        );

        // filter by labels
        let mut map = HashMap::new();
        map.entry("im.lain.docker-checker".to_string())
            .or_insert("skipme".to_string());
        assert!(
            !dc.filter_containers(&create_mock_container(None, Some("filter_me".to_string()), Some(map))),
            "Should be filtered by label!"
        );
    }
}
