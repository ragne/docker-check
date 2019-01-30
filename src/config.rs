use serde::{Deserialize, Deserializer};

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

#[derive(Debug, PartialEq)]
pub enum ApplyTo {
  Name,
  Image,
  Both,
  DontApply
}

impl<'de> Deserialize<'de> for ApplyTo {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let v = Vec::<String>::deserialize(deserializer)?;
        Ok(ApplyTo::from_vec(&v).unwrap())
    }
}

impl ApplyTo {
    fn from_vec(v: &Vec<String>) -> std::result::Result<Self, ()> {
        if v.len() == 0 {
          return Ok(ApplyTo::DontApply)
        };
        let mut last = None;
        v.iter().for_each(|x| 
          match x.as_str() {
            "name" => {
              if last.is_none() {
                last = Some(ApplyTo::Name)
              } else if last.is_some() && last.take().unwrap() == ApplyTo::Image {
                last = Some(ApplyTo::Both)
              }
            },
            "image" => {
              if last.is_none() {
                last = Some(ApplyTo::Image)
              } else if last.is_some() && last.take().unwrap() == ApplyTo::Name {
                last = Some(ApplyTo::Both)
              }
            },
            _ => { last = Some(ApplyTo::DontApply)}
          }
        );
        last.ok_or(())
    }

    pub fn should_filter_images(&self) -> bool {
        *self == ApplyTo::Image || *self == ApplyTo::Both
    }

    pub fn should_filter_names(&self) -> bool {
        *self == ApplyTo::Name || *self == ApplyTo::Both
    }
}


#[derive(Debug, Deserialize)]
pub struct ContainersConfig {
  pub filter_by: String,
  pub apply_filter_to: ApplyTo,
  pub consecutive_failures: u16,
  pub hard_failures: u16,
  pub run_on_failure: String
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
    let config_str = include_str!("../settings.toml");
    let mut settings = configuration::Config::default();
    settings.merge(configuration::File::from_str(config_str,configuration::FileFormat::Toml)).unwrap();
    settings.try_into::<Config>().unwrap()
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
    assert_eq!(ApplyTo::from_vec(&v).unwrap(), ApplyTo::DontApply);
    v.push("image".to_string());
    assert_eq!(ApplyTo::from_vec(&v).unwrap(), ApplyTo::Image);
    v.push("name".to_string());
    assert_eq!(ApplyTo::from_vec(&v).unwrap(), ApplyTo::Both);
    v.remove(0);
    assert_eq!(ApplyTo::from_vec(&v).unwrap(), ApplyTo::Name);
  }
}