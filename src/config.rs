use dotenvy::dotenv;
use serde::{Deserialize, Serialize};

use crate::error_messages::TextEgressError;

fn load_env() {
    match dotenv() {
        Ok(_) => {}
        Err(e) => {
            log::error!(
                "Failed to load .env file: {}, assuming variables are set",
                e
            );
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEgressConfig {
    pub syncflow_server_url: String,
    pub projects: Vec<Projects>,
    pub rabbitmq_host: String,
    pub rabbitmq_port: u16,
    pub rabbitmq_vhost_name: String,
    pub device_group_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projects {
    pub key: String,
    pub secret: String,
    pub project_id: String,
    pub s3_config: S3Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub access_key: String,
    pub secret_key: String,
    pub bucket_name: String,
    pub endpoint: String,
    pub region: String,
}

impl TextEgressConfig {
    pub fn load() -> Result<Self, TextEgressError> {
        load_env();
        let config = envious::Config::default().build_from_env::<TextEgressConfig>()?;
        Ok(config)
    }

    pub fn from_filename(file_path: &str) -> Result<Self, TextEgressError> {
        let _ = dotenvy::from_filename(file_path)?;
        let config = envious::Config::default().build_from_env::<TextEgressConfig>()?;
        Ok(config)
    }
}
