use gtk::gio;
use gtk::prelude::*;
use std::path::PathBuf;

/// URI scheme used for sidebar rows of volumes that are not mounted yet.
pub const DEVICE_URI: &str = "device://";

/// A local storage volume known to GIO (USB stick, SD card, disc, partition).
#[derive(Debug, Clone)]
pub struct DeviceVolume {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// Mount root when the volume is mounted.
    pub mount_path: Option<PathBuf>,
}

impl DeviceVolume {
    /// Sidebar path: the mount root, or a `device://` placeholder that mounts on click.
    pub fn sidebar_path(&self) -> PathBuf {
        self.mount_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("{DEVICE_URI}{}", self.id)))
    }
}

fn volume_id(volume: &gio::Volume) -> String {
    volume
        .identifier(gio::VOLUME_IDENTIFIER_KIND_UUID)
        .or_else(|| volume.identifier(gio::VOLUME_IDENTIFIER_KIND_UNIX_DEVICE))
        .map(|s| s.to_string())
        .unwrap_or_else(|| volume.name().to_string())
}

/// Lists the non-network volumes reported by the GIO volume monitor.
///
/// Must be called on the main thread.
pub fn list_volumes() -> Vec<DeviceVolume> {
    gio::VolumeMonitor::get()
        .volumes()
        .into_iter()
        .filter(|v| v.identifier(gio::VOLUME_IDENTIFIER_KIND_CLASS).as_deref() != Some("network"))
        .map(|v| {
            let icon = v
                .symbolic_icon()
                .downcast::<gio::ThemedIcon>()
                .ok()
                .and_then(|themed| themed.names().first().map(|s| s.to_string()))
                .unwrap_or_else(|| "drive-removable-media-symbolic".to_owned());
            DeviceVolume {
                id: volume_id(&v),
                name: v.name().to_string(),
                icon,
                mount_path: v.get_mount().and_then(|m| m.root().path()),
            }
        })
        .collect()
}

/// Finds the volume whose [`DeviceVolume::id`] is `id`.
pub fn find_volume(id: &str) -> Option<gio::Volume> {
    gio::VolumeMonitor::get()
        .volumes()
        .into_iter()
        .find(|v| volume_id(v) == id)
}
