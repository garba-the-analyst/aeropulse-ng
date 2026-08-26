//! SDR device registry with strict USB serial-port locking.
//!
//! Three receiver channels are bound to fixed hardware serial numbers at
//! boot so that channel assignment never drifts across reboots or USB port
//! changes. Enumeration is abstracted behind [`UsbProbe`] so production
//! hosts use the libusb backend while bench/air-gapped development falls
//! back to a deterministic simulated inventory.

use crate::models::HardwareStatus;
use serde::{Deserialize, Serialize};

/// Logical receiver roles inside the AeroPulse ingestion topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdrRole {
    /// 1090 MHz Mode S / ADS-B ES.
    AdsB,
    /// 131.550 MHz VHF ACARS.
    Acars,
    /// VHF guard / tactical scan channel.
    TacScan,
}

impl SdrRole {
    #[inline]
    pub fn channel_id(self) -> &'static str {
        match self {
            SdrRole::AdsB => "SDR-1",
            SdrRole::Acars => "SDR-2",
            SdrRole::TacScan => "SDR-3",
        }
    }

    #[inline]
    pub fn default_frequency_hz(self) -> u32 {
        match self {
            SdrRole::AdsB => 1_090_000_000,
            SdrRole::Acars => 131_550_000,
            SdrRole::TacScan => 121_500_000,
        }
    }

    pub fn role_name(self) -> &'static str {
        match self {
            SdrRole::AdsB => "1090MHz ADS-B",
            SdrRole::Acars => "ACARS",
            SdrRole::TacScan => "VHF GUARD",
        }
    }
}

/// A physical RTL-SDR dongle discovered on the bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsbDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    /// Factory-burned serial string used for deterministic binding.
    pub serial: String,
    pub bus: u8,
    pub port: u8,
}

/// Hardware enumeration backend.
pub trait UsbProbe: Send + Sync {
    fn scan(&self) -> Vec<UsbDevice>;
}

/// Production probe: enumerates RTL2832U devices (RTL-SDR Blog v4 /
/// FlightAware Pro Stick) via libusb hotplug descriptors.
pub struct LibUsbProbe;

impl UsbProbe for LibUsbProbe {
    fn scan(&self) -> Vec<UsbDevice> {
        // The rtlsdr sysfs interface is probed first; when absent (air-gapped
        // workstation without driver bindings) an empty inventory is returned
        // and the engine degrades to the synthetic feed.
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
            for entry in entries.flatten() {
                let path = entry.path();
                let id_vendor = std::fs::read_to_string(path.join("idVendor")).unwrap_or_default();
                let id_product =
                    std::fs::read_to_string(path.join("idProduct")).unwrap_or_default();
                if !(id_vendor.trim() == "0bda" && ["0x8832", "0x2838", "0x2832"].contains(&id_product.trim())) {
                    continue;
                }
                let serial = std::fs::read_to_string(path.join("serial"))
                    .unwrap_or_else(|_| format!("unbound-{}", entry.file_name().to_string_lossy()))
                    .trim()
                    .to_string();
                let port = entry
                    .file_name()
                    .to_string_lossy()
                    .chars()
                    .last()
                    .and_then(|c| c.to_digit(10))
                    .unwrap_or(0) as u8;
                found.push(UsbDevice {
                    vendor_id: u16::from_str_radix(id_vendor.trim().trim_start_matches("0x"), 16)
                        .unwrap_or(0x0bda),
                    product_id: u16::from_str_radix(
                        id_product.trim().trim_start_matches("0x"),
                        16,
                    )
                    .unwrap_or(0),
                    serial,
                    bus: 1,
                    port,
                });
            }
        }
        found.sort_by(|a, b| a.serial.cmp(&b.serial));
        found
    }
}

/// Bench fallback producing a stable pseudo-inventory.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimulatedProbe;

impl UsbProbe for SimulatedProbe {
    fn scan(&self) -> Vec<UsbDevice> {
        vec![
            UsbDevice {
                vendor_id: 0x0bda,
                product_id: 0x8832,
                serial: "00000001".into(),
                bus: 1,
                port: 1,
            },
            UsbDevice {
                vendor_id: 0x0bda,
                product_id: 0x8832,
                serial: "00000002".into(),
                bus: 1,
                port: 3,
            },
            UsbDevice {
                vendor_id: 0x0bda,
                product_id: 0x8832,
                serial: "00000003".into(),
                bus: 1,
                port: 5,
            },
        ]
    }
}

/// Static channel plan: role -> required dongle serial. Edit per airframe
/// installation; the registry refuses to start mismatched hardware in
/// enforcement mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPlan {
    pub bindings: Vec<(SdrRole, String)>,
    pub sample_rate_sps: u32,
    pub gain_db: f32,
}

impl Default for ChannelPlan {
    fn default() -> Self {
        Self {
            bindings: vec![
                (SdrRole::AdsB, "00000001".into()),
                (SdrRole::Acars, "00000002".into()),
                (SdrRole::TacScan, "00000003".into()),
            ],
            sample_rate_sps: 2_400_000,
            gain_db: 42.0,
        }
    }
}

/// Resolved runtime view of one channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundChannel {
    pub role: SdrRole,
    pub status: HardwareStatus,
}

/// Validates `plan` against `devices`, binding each role to its locked serial.
pub fn bind_channels(plan: &ChannelPlan, devices: &[UsbDevice]) -> Vec<BoundChannel> {
    plan.bindings
        .iter()
        .map(|(role, serial)| {
            let matched = devices.iter().find(|d| &d.serial == serial);
            BoundChannel {
                role: *role,
                status: HardwareStatus {
                    channel_id: role.channel_id().to_string(),
                    role: role.role_name().to_string(),
                    serial_lock: serial.clone(),
                    frequency_hz: role.default_frequency_hz(),
                    sample_rate_sps: plan.sample_rate_sps,
                    gain_db: plan.gain_db,
                    messages_per_second: 0.0,
                    ppm_error: 0.0,
                    online: matched.is_some(),
                    usb_port: matched.map(|d| format!("bus{}-port{}", d.bus, d.port)),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_matching_serials() {
        let plan = ChannelPlan::default();
        let devices = SimulatedProbe.scan();
        let channels = bind_channels(&plan, &devices);
        assert_eq!(channels.len(), 3);
        assert!(channels.iter().all(|c| c.status.online));
        assert_eq!(channels[0].status.frequency_hz, 1_090_000_000);
    }

    #[test]
    fn missing_dongle_reports_offline() {
        let plan = ChannelPlan::default();
        let channels = bind_channels(&plan, &[]);
        assert!(channels.iter().all(|c| !c.status.online));
    }

    #[test]
    fn sysfs_probe_never_panics() {
        let probe = LibUsbProbe;
        let _ = probe.scan();
    }
}
