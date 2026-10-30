//! The facade every consumer uses: one method per documented action.
//!
//! Nothing above this layer knows whether it is talking to UCloud or to
//! `fixtures/mock`; [`UcloudClient::mode`] exists only for the UI badge.

use crate::error::{Error, Result};
use crate::models::{
    Balance, BillList, EipList, RegionList, UDiskList, UHostList, UMemList, UdbList, UlbList,
};
use crate::paths::Action;
use crate::transport::{Backend, mock_enabled};

/// A configured UCloud account.
#[derive(Debug)]
pub struct UcloudClient {
    backend: Backend,
    /// Default region used when a call does not name one.
    pub region: Option<String>,
}

/// Which backend is in use, for the title-bar badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Real account; mutations cost money.
    Live,
    /// Fixtures; nothing leaves this machine.
    Mock,
}

impl UcloudClient {
    /// Live client from `UCLOUD_*` environment variables.
    pub fn live() -> Result<Self> {
        let backend = Backend::live_from_env()?;
        let region = std::env::var("UCLOUD_REGION")
            .ok()
            .filter(|v| !v.is_empty());
        Ok(Self { backend, region })
    }

    /// Fixture client.
    pub fn mock() -> Self {
        Self {
            backend: Backend::mock(),
            region: Some("cn-bj2".to_owned()),
        }
    }

    /// Mock when `UDASH_MOCK=1`, live otherwise.
    pub fn from_env() -> Result<Self> {
        if mock_enabled() {
            Ok(Self::mock())
        } else {
            Self::live()
        }
    }

    /// Which backend this client talks to.
    pub fn mode(&self) -> Mode {
        if self.backend.is_mock() {
            Mode::Mock
        } else {
            Mode::Live
        }
    }

    /// Raw transport access, for actions that are not modelled yet.
    pub fn call(&self, action: &str, params: &[(String, String)]) -> Result<serde_json::Value> {
        self.backend.call(action, params)
    }

    fn region_params(&self) -> Vec<(String, String)> {
        self.region
            .as_ref()
            .map(|region| vec![("Region".to_owned(), region.clone())])
            .unwrap_or_default()
    }

    fn decode<T: serde::de::DeserializeOwned>(
        &self,
        action: &str,
        value: serde_json::Value,
    ) -> Result<T> {
        serde_json::from_value(value).map_err(|error| Error::Decode(format!("{action}: {error}")))
    }

    /// Account balance.
    pub fn balance(&self) -> Result<Balance> {
        let value = self.backend.call(Action::GET_BALANCE, &[])?;
        self.decode(Action::GET_BALANCE, value)
    }

    /// All regions.
    pub fn regions(&self) -> Result<RegionList> {
        let value = self.backend.call(Action::GET_REGION, &[])?;
        self.decode(Action::GET_REGION, value)
    }

    /// Cloud hosts in every region.
    pub fn uhosts(&self) -> Result<UHostList> {
        let mut params = self.region_params();
        params.push(("Limit".to_owned(), "100".to_owned()));
        let value = self.backend.call(Action::DESCRIBE_UHOST, &params)?;
        self.decode(Action::DESCRIBE_UHOST, value)
    }

    /// Elastic IPs.
    pub fn eips(&self) -> Result<EipList> {
        let mut params = self.region_params();
        params.push(("Limit".to_owned(), "100".to_owned()));
        let value = self.backend.call(Action::DESCRIBE_EIP, &params)?;
        self.decode(Action::DESCRIBE_EIP, value)
    }

    /// Cloud disks.
    pub fn udisks(&self) -> Result<UDiskList> {
        let mut params = self.region_params();
        params.push(("Limit".to_owned(), "100".to_owned()));
        let value = self.backend.call(Action::DESCRIBE_UDISK, &params)?;
        self.decode(Action::DESCRIBE_UDISK, value)
    }

    /// Managed databases.
    pub fn udbs(&self) -> Result<UdbList> {
        let mut params = self.region_params();
        params.push(("Limit".to_owned(), "100".to_owned()));
        let value = self.backend.call(Action::DESCRIBE_UDB, &params)?;
        self.decode(Action::DESCRIBE_UDB, value)
    }

    /// Managed Redis.
    pub fn redis(&self) -> Result<UMemList> {
        let mut params = self.region_params();
        params.push(("Limit".to_owned(), "100".to_owned()));
        let value = self.backend.call(Action::DESCRIBE_UMEM, &params)?;
        self.decode(Action::DESCRIBE_UMEM, value)
    }

    /// Load balancers.
    pub fn ulbs(&self) -> Result<UlbList> {
        let mut params = self.region_params();
        params.push(("Limit".to_owned(), "100".to_owned()));
        let value = self.backend.call(Action::DESCRIBE_ULB, &params)?;
        self.decode(Action::DESCRIBE_ULB, value)
    }

    /// Bill entries for the current month.
    pub fn bills(&self) -> Result<BillList> {
        let value = self.backend.call(Action::LIST_BILLS, &[])?;
        self.decode(Action::LIST_BILLS, value)
    }
}
