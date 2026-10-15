pub mod bailian_cli;
pub mod claude;
pub mod codex;
pub mod codex_http;
pub mod deepseek;
pub mod plans;
pub mod registry;
mod subscription;
use crate::domain::models::{ProviderDescriptor, QuotaMetric};
use std::{future::Future, pin::Pin};
#[derive(Clone, Debug)]
pub struct FetchError {
    pub code: &'static str,
    pub retry_after_seconds: u64,
}
impl FetchError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            retry_after_seconds: 60,
        }
    }
}
pub type FetchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<QuotaMetric>, FetchError>> + Send + 'a>>;
pub trait Provider: Send + Sync {
    fn requires_credential(&self) -> bool {
        true
    }
    fn network_kind(&self) -> &'static str {
        "http_client"
    }
    fn fetch_routed<'a>(
        &'a self,
        client: &'a reqwest::Client,
        url: &'a str,
        key: &'a str,
        _: &'a crate::services::network::NetworkConfig,
    ) -> FetchFuture<'a> {
        self.fetch(client, url, key)
    }
    fn descriptor(&self) -> ProviderDescriptor;
    fn validate_url(&self, url: &str) -> Result<(), String>;
    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
        url: &'a str,
        key: &'a str,
    ) -> FetchFuture<'a>;
}
