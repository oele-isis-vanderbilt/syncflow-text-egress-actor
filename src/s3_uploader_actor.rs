use crate::text_egress_actor::{S3UploaderUpdates, TextEgressActor};
use actix::{Actor, Addr, Handler, Message};
use rusoto_s3::S3;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum S3UploaderMessages {
    Start { prefix: String, files: Vec<String> },
}

pub(crate) struct S3UploaderActor {
    egress_id: String,
    bucket: String,
    s3_client: rusoto_s3::S3Client,
    parent_addr: Addr<TextEgressActor>,
}

impl Actor for S3UploaderActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("S3UploaderActor started");
    }
}

impl S3UploaderActor {
    pub fn new(
        egress_id: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
        region: &str,
        endpoint: &str,
        parent_addr: Addr<TextEgressActor>,
    ) -> Self {
        let client = rusoto_core::HttpClient::new().expect("Failed to create request dispatcher");
        let region = rusoto_core::Region::Custom {
            name: region.to_string(),
            endpoint: endpoint.to_string(),
        };
        let credentials_provider = rusoto_credential::StaticProvider::new_minimal(
            access_key.to_string(),
            secret_key.to_string(),
        );

        let s3_client = rusoto_s3::S3Client::new_with(client, credentials_provider, region);

        S3UploaderActor {
            egress_id: egress_id.to_string(),
            bucket: bucket.to_string(),
            s3_client,
            parent_addr,
        }
    }
}

impl Handler<S3UploaderMessages> for S3UploaderActor {
    type Result = ();

    fn handle(&mut self, msg: S3UploaderMessages, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            S3UploaderMessages::Start { prefix, files } => {
                log::info!("Uploading files to S3 bucket: {}", self.bucket);
                let parent_addr = self.parent_addr.clone();
                let bucket = self.bucket.clone();
                let egress_id = self.egress_id.clone();
                let s3_client = self.s3_client.clone();
                let mut uploaded_files: Vec<String> = vec![];

                actix::spawn(async move {
                    for file in files {
                        log::info!("Uploading file: {}", &file);
                        let mut fh = match tokio::fs::File::open(&file).await {
                            Ok(f) => f,
                            Err(e) => {
                                log::error!("Failed to open file: {:?}", e);
                                parent_addr.do_send(S3UploaderUpdates::Failed {
                                    egress_id: egress_id.clone(),
                                    error: e.into(),
                                });
                                return ();
                            }
                        };
                        let file_path = PathBuf::from(&file);
                        let file_name = file_path.file_name().unwrap().to_str().unwrap();
                        let mut buffer = Vec::new();
                        match fh.read_to_end(&mut buffer).await {
                            Ok(_) => {}
                            Err(e) => {
                                parent_addr.do_send(S3UploaderUpdates::Failed {
                                    egress_id: egress_id.clone(),
                                    error: e.into(),
                                });
                                return ();
                            }
                        }
                        let key = format!("{}/{}/{}", bucket.clone(), prefix, file_name);
                        let put_req = rusoto_s3::PutObjectRequest {
                            bucket: bucket.clone(),
                            key: key.clone(),
                            body: Some(buffer.into()),
                            content_type: Some("text/plain".to_string()),
                            ..Default::default()
                        };
                        let res = s3_client.put_object(put_req).await;
                        match res {
                            Ok(_) => {
                                uploaded_files.push(key.clone());
                            }
                            Err(e) => {
                                log::error!("Failed to upload file: {:?}", e);
                                parent_addr.do_send(S3UploaderUpdates::Failed {
                                    egress_id: egress_id.clone(),
                                    error: e.into(),
                                });
                                return ();
                            }
                        }
                    }
                    parent_addr.do_send(S3UploaderUpdates::Completed {
                        egress_id,
                        bucket: bucket.clone(),
                        files: uploaded_files,
                    });
                });
            }
        }
    }
}
