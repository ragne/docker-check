use serde::{Deserialize, Deserializer};
use std::time::{Duration};

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
  pub checker: String,
  pub default: String,
}

#[derive(Debug, Deserialize)]
pub struct DockerConfig {
  pub connect_uri: String,
  pub tls: bool,
  pub purge_unseen: u64,
}

#[derive(Debug, PartialEq)]
pub struct ApplyTo {
  // Name,
  // Image,
  // Label,
  // Both,
  // All,
  // DontApply,
  mask: u8
}

impl<'de> Deserialize<'de> for ApplyTo {
  fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let v = Vec::<String>::deserialize(deserializer)?;
    Ok(ApplyTo::from_vec(&v).unwrap())
  }
}

impl ApplyTo {
    #[inline]
    fn match_bit(&self, idx: u8) -> bool {
      self.mask >> idx & 1 == 1
    }

  fn from_vec(v: &Vec<String>) -> std::result::Result<Self, ()> {
    let mut apply_bytes: u8 = 0b0000_0000;
    // Name, image, label
    v.iter().for_each(|x| match x.as_str() {
      "name" => {
        apply_bytes |= 0b0000_0001;
      }
      "image" => {
        apply_bytes |= 0b0000_0010;
      },
      "label" => {
        apply_bytes |= 0b0000_0100;
      }
      _ => {},
    });

    Ok(Self{ mask: apply_bytes})
  }

  pub fn should_filter_images(&self) -> bool {
    self.match_bit(1)
  }

  pub fn should_filter_names(&self) -> bool {
    self.match_bit(0)
  }

  pub fn should_filter_labels(&self) -> bool {
    self.match_bit(2)
  }
}

#[derive(Debug, Deserialize)]
pub struct ContainersConfig {
  pub filter_by: String,
  pub apply_filter_to: ApplyTo,
  pub consecutive_failures: u16,
  pub hard_failures: u16,
  pub run_on_failure: String,
  pub filter_self: String
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
  pub aws: AwsConfig,
}

pub fn get_settings() -> Result<Config, String> {
  let mut settings = configuration::Config::default();
  settings
    // Add in `./Settings.toml`
    .merge(configuration::File::with_name("settings"))
    .map_err(|e| e.to_string())?
    // Add in settings from the environment (with a prefix of APP)
    // Eg.. `APP_DEBUG=1 ./target/app` would set the `debug` key
    .merge(configuration::Environment::with_prefix("APP"))
    .map_err(|e| e.to_string())?;

  Ok(
    settings
      .try_into::<Config>()
      .map_err(|e| format!("Cannot parse config correctly! Nested error: {}", e.to_string()))?,
  )
}

impl Config {
  fn default() -> Result<Self, String> {
    let config_str = include_str!("../settings.toml");
    let mut settings = configuration::Config::default();
    settings
      .merge(configuration::File::from_str(
        config_str,
        configuration::FileFormat::Toml,
      )).map_err(|e| e.to_string())?;
    Ok(settings.try_into::<Config>().map_err(|e| e.to_string())?)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn create_config_from_default() {
    let _ = Config::default();
  }

  #[test]
  fn get_settings_should_work() {
    get_settings().unwrap();
  }

  #[test]
  fn apply_to_test() {
    let mut v = Vec::new();
    let mut apply_to = ApplyTo::from_vec(&v).unwrap();
    assert!(apply_to.should_filter_images() == false);
    assert!(apply_to.mask == 0b0000_0000);
    v.push("image".to_string());
    apply_to = ApplyTo::from_vec(&v).unwrap();
    println!("hha. {:#08b}, res: {}", apply_to.mask, apply_to.match_bit(6));
    assert!(apply_to.should_filter_images() == true);
    assert!(apply_to.mask == 0b0000_0010);
    v.push("name".to_string());
    apply_to = ApplyTo::from_vec(&v).unwrap();
    assert!(apply_to.should_filter_images() == true);
    assert!(apply_to.should_filter_names() == true);
    assert!(apply_to.mask == 0b0000_0011);
    v.remove(0);
    v.push("label".to_string());
    apply_to = ApplyTo::from_vec(&v).unwrap();
    assert!(apply_to.should_filter_names() == true);
    assert!(apply_to.should_filter_labels() == true);
  }
}
