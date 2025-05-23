//! Remote Service Discovery
//! Communicates via XPC and returns advertised services

use std::collections::HashMap;

use log::warn;
use serde::Deserialize;

use crate::{IdeviceError, ReadWrite, RemoteXpcClient};

/// Describes an available XPC service
#[derive(Debug, Clone, Deserialize)]
pub struct RsdService {
    /// Required entitlement to access this service
    pub entitlement: String,
    /// Port number where the service is available
    pub port: u16,
    /// Whether the service uses remote XPC
    pub uses_remote_xpc: bool,
    /// Optional list of supported features
    pub features: Option<Vec<String>>,
    /// Optional service version number
    pub service_version: Option<i64>,
}

pub struct RsdClient<R: ReadWrite> {
    inner: RemoteXpcClient<R>,
}

impl<R: ReadWrite> RsdClient<R> {
    pub async fn new(socket: R) -> Result<Self, IdeviceError> {
        Ok(Self {
            inner: RemoteXpcClient::new(socket).await?,
        })
    }

    pub async fn get_services(&mut self) -> Result<HashMap<String, RsdService>, IdeviceError> {
        let data = self.inner.do_handshake().await?;

        let data = match data
            .as_dictionary()
            .and_then(|x| x.get("Services"))
            .and_then(|x| x.as_dictionary())
        {
            Some(d) => d,
            None => return Err(IdeviceError::UnexpectedResponse),
        };

        // Parse available services
        let mut services = HashMap::new();
        for (name, service) in data.into_iter() {
            match service.as_dictionary() {
                Some(service) => {
                    let entitlement = match service.get("Entitlement").and_then(|x| x.as_string()) {
                        Some(e) => e.to_string(),
                        None => {
                            warn!("Service did not contain entitlement string");
                            continue;
                        }
                    };
                    let port = match service
                        .get("Port")
                        .and_then(|x| x.as_string())
                        .and_then(|x| x.parse::<u16>().ok())
                    {
                        Some(e) => e,
                        None => {
                            warn!("Service did not contain port string");
                            continue;
                        }
                    };
                    let uses_remote_xpc = match service
                        .get("Properties")
                        .and_then(|x| x.as_dictionary())
                        .and_then(|x| x.get("UsesRemoteXPC"))
                        .and_then(|x| x.as_boolean())
                    {
                        Some(e) => e.to_owned(),
                        None => false, // default is false
                    };

                    let features = service
                        .get("Properties")
                        .and_then(|x| x.as_dictionary())
                        .and_then(|x| x.get("Features"))
                        .and_then(|x| x.as_array())
                        .map(|f| {
                            f.iter()
                                .filter_map(|x| x.as_string())
                                .map(|x| x.to_string())
                                .collect::<Vec<String>>()
                        });

                    let service_version = service
                        .get("Properties")
                        .and_then(|x| x.as_dictionary())
                        .and_then(|x| x.get("ServiceVersion"))
                        .and_then(|x| x.as_signed_integer())
                        .map(|e| e.to_owned());

                    services.insert(
                        name.to_string(),
                        RsdService {
                            entitlement,
                            port,
                            uses_remote_xpc,
                            features,
                            service_version,
                        },
                    );
                }
                None => {
                    warn!("Service is not a dictionary!");
                    continue;
                }
            }
        }

        Ok(services)
    }
}
