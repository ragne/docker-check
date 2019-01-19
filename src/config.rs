
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
  pub checker: String,
  pub default: String
}

#[derive(Debug, Deserialize)]
pub struct DockerConfig {
  pub connect_uri: String,
  pub tls: bool
}


#[derive(Debug, Deserialize)]
pub struct ContainersConfig {
  pub filter_by: String,
  pub consecutive_failures: u16,
  pub hard_failures: u16,
}

#[derive(Debug, Deserialize)]
pub struct AwsAsgConfig {
  pub healthcheck: bool,
}

#[derive(Debug, Deserialize)]
pub struct AwsConfig {
  pub enabled: bool,
  pub asg: AwsAsgConfig,
}

#[derive(Debug, Deserialize)]
pub struct Config {
  pub logging: LoggingConfig,
  pub docker: DockerConfig,
  pub containers: ContainersConfig,
  pub aws: AwsConfig
}

pub fn get_settings() -> Result<Config, String> {
let mut settings = configuration::Config::default();
    settings
        // Add in `./Settings.toml`
        .merge(configuration::File::with_name("settings")).map_err(|_| "Cannot find the settings file")?
        // Add in settings from the environment (with a prefix of APP)
        // Eg.. `APP_DEBUG=1 ./target/app` would set the `debug` key
        .merge(configuration::Environment::with_prefix("APP")).map_err(|e| e.to_string())?;

    Ok(settings.try_into::<Config>().map_err(|e| format!("Cannot parse config correctly! Nested error: {}", e.to_string()))?)
}

impl Default for Config {
  fn default() -> Self {
    let config_str = r###"
[logging]
checker = "debug"
default = "warn"

[docker]
connect_uri = "unix:///var/run/docker.sock"
tls = true

[containers]
filter_by = ".*"
consecutive_failures = 5
hard_failures = 3

[aws]
enabled = true
[aws.asg]
healthcheck = true"###;
    let mut settings = configuration::Config::default();
    settings.merge(configuration::File::from_str(config_str,configuration::FileFormat::Toml)).unwrap();
    settings.try_into::<Config>().unwrap()
  }
}