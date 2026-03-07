use std::collections::HashMap;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

/// Service record for a discovered QuicView host on the LAN.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRecord {
    /// Human-readable host name.
    pub host_name: String,
    /// Socket address (IP + port).
    pub addr: SocketAddr,
    /// Role advertised by the host (e.g. "host", "extender").
    pub role: String,
}

/// Trait for LAN service discovery (mDNS / DNS-SD).
pub trait Discovery: Send {
    /// Advertise this instance on the network.
    fn advertise(&mut self, record: &ServiceRecord) -> Result<(), DiscoveryError>;

    /// Stop advertising.
    fn stop_advertise(&mut self) -> Result<(), DiscoveryError>;

    /// Discover all QuicView services on the LAN.
    fn discover(&self) -> Result<Vec<ServiceRecord>, DiscoveryError>;
}

/// Errors from the discovery subsystem.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("discovery failed: {0}")]
    Failed(String),

    #[error("platform not supported")]
    PlatformNotSupported,
}

/// In-memory discovery for testing that simulates a LAN.
pub struct MemoryDiscovery {
    services: HashMap<SocketAddr, ServiceRecord>,
    advertised: Option<ServiceRecord>,
}

impl MemoryDiscovery {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            advertised: None,
        }
    }

    /// Simulate another host appearing on the network.
    pub fn inject_service(&mut self, record: ServiceRecord) {
        self.services.insert(record.addr, record);
    }

    /// Remove a simulated host.
    pub fn remove_service(&mut self, addr: &SocketAddr) {
        self.services.remove(addr);
    }
}

impl Default for MemoryDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl Discovery for MemoryDiscovery {
    fn advertise(&mut self, record: &ServiceRecord) -> Result<(), DiscoveryError> {
        self.advertised = Some(record.clone());
        self.services.insert(record.addr, record.clone());
        Ok(())
    }

    fn stop_advertise(&mut self) -> Result<(), DiscoveryError> {
        if let Some(rec) = self.advertised.take() {
            self.services.remove(&rec.addr);
        }
        Ok(())
    }

    fn discover(&self) -> Result<Vec<ServiceRecord>, DiscoveryError> {
        Ok(self.services.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_discovery_advertise_and_find() {
        let mut disc = MemoryDiscovery::new();
        let record = ServiceRecord {
            host_name: "devbox".into(),
            addr: "192.168.1.5:4433".parse().unwrap(),
            role: "host".into(),
        };

        disc.advertise(&record).unwrap();
        let found = disc.discover().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].host_name, "devbox");
    }

    #[test]
    fn memory_discovery_multiple_services() {
        let mut disc = MemoryDiscovery::new();
        disc.inject_service(ServiceRecord {
            host_name: "host-a".into(),
            addr: "192.168.1.10:4433".parse().unwrap(),
            role: "host".into(),
        });
        disc.inject_service(ServiceRecord {
            host_name: "rpi-1".into(),
            addr: "192.168.1.20:4433".parse().unwrap(),
            role: "extender".into(),
        });

        let found = disc.discover().unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn memory_discovery_stop_advertise() {
        let mut disc = MemoryDiscovery::new();
        let record = ServiceRecord {
            host_name: "test".into(),
            addr: "10.0.0.1:4433".parse().unwrap(),
            role: "host".into(),
        };
        disc.advertise(&record).unwrap();
        assert_eq!(disc.discover().unwrap().len(), 1);

        disc.stop_advertise().unwrap();
        assert_eq!(disc.discover().unwrap().len(), 0);
    }
}
