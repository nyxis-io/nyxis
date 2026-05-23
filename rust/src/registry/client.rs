use std::time::Duration;

use tonic::transport::{Channel, Endpoint};
use tonic::Request;

use super::pb::registry_service_client::RegistryServiceClient;
use super::pb::{
    GetSchemaByHashRequest, GetSchemaByHashResponse, ListSchemasRequest, ListSchemasResponse,
    RegisterSchemaRequest, RegisterSchemaResponse,
};

pub struct RegistryClient {
    inner: RegistryServiceClient<Channel>,
}

impl RegistryClient {
    pub async fn connect(server: &str) -> Result<Self, String> {
        let uri = if server.starts_with("http://") || server.starts_with("https://") {
            server.to_string()
        } else {
            format!("http://{server}")
        };
        let endpoint = Endpoint::from_shared(uri)
            .map_err(|e| format!("invalid server address: {e}"))?
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30));
        let channel = endpoint
            .connect()
            .await
            .map_err(|e| format!("connect to registry failed: {e}"))?;
        Ok(Self {
            inner: RegistryServiceClient::new(channel),
        })
    }

    pub async fn register_schema(
        &mut self,
        dict_hash: [u8; 8],
        schema_bytes: Vec<u8>,
        drift_policy: &str,
    ) -> Result<RegisterSchemaResponse, String> {
        let req = RegisterSchemaRequest {
            dict_hash: dict_hash.to_vec(),
            schema_bytes,
            drift_policy: drift_policy.to_string(),
        };
        self.inner
            .register_schema(Request::new(req))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| format!("RegisterSchema: {}", s.message()))
    }

    pub async fn get_schema_by_hash(
        &mut self,
        dict_hash: [u8; 8],
    ) -> Result<GetSchemaByHashResponse, String> {
        let req = GetSchemaByHashRequest {
            dict_hash: dict_hash.to_vec(),
        };
        self.inner
            .get_schema_by_hash(Request::new(req))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| format!("GetSchemaByHash: {}", s.message()))
    }

    pub async fn list_schemas(
        &mut self,
        limit: u32,
        offset: u32,
    ) -> Result<ListSchemasResponse, String> {
        let req = ListSchemasRequest { limit, offset };
        self.inner
            .list_schemas(Request::new(req))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| format!("ListSchemas: {}", s.message()))
    }
}
