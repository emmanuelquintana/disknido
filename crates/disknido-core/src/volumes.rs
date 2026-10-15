//! Volume discovery: letters, labels, file system, capacity, media hints.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    RamDisk,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Media {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    /// `C:\`
    pub root: String,
    pub letter: String,
    pub label: String,
    pub file_system: String,
    pub kind: DriveKind,
    pub media: Media,
    /// USB / removable bus: shown as "external" in the UI.
    pub external: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// `\\?\Volume{GUID}\` when the system reports it.
    pub volume_id: Option<String>,
    /// Physical disk number, so the UI never assumes two letters are two devices.
    pub disk_number: Option<u32>,
    pub available: bool,
}

#[cfg(windows)]
pub fn list_volumes() -> Vec<Volume> {
    use windows_sys::Win32::Storage::FileSystem::*;
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let wroot: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
        let kind = match unsafe { GetDriveTypeW(wroot.as_ptr()) } {
            2 => DriveKind::Removable,
            3 => DriveKind::Fixed,
            4 => DriveKind::Network,
            5 => DriveKind::Optical,
            6 => DriveKind::RamDisk,
            _ => DriveKind::Unknown,
        };
        let mut label = [0u16; 261];
        let mut fs = [0u16; 261];
        let ok = unsafe {
            GetVolumeInformationW(
                wroot.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        } != 0;
        let wstr = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
        let (free, total) = if ok { crate::fsx::free_space(std::path::Path::new(&root)).unwrap_or((0, 0)) } else { (0, 0) };
        let mut guid = [0u16; 64];
        let volume_id = (unsafe { GetVolumeNameForVolumeMountPointW(wroot.as_ptr(), guid.as_mut_ptr(), guid.len() as u32) } != 0)
            .then(|| wstr(&guid));
        let (media, usb, disk_number) = if kind == DriveKind::Fixed || kind == DriveKind::Removable {
            device_info(letter)
        } else {
            (Media::Unknown, false, None)
        };
        out.push(Volume {
            letter: format!("{letter}:"),
            root,
            label: if ok { wstr(&label) } else { String::new() },
            file_system: if ok { wstr(&fs) } else { String::new() },
            external: usb || kind == DriveKind::Removable,
            kind,
            media,
            total_bytes: total,
            free_bytes: free,
            volume_id,
            disk_number,
            available: ok,
        });
    }
    out
}

/// Seek penalty (HDD vs SSD), bus type and disk number via
/// IOCTL_STORAGE_QUERY_PROPERTY. Opening the volume with zero access rights
/// does not require administrator privileges.
#[cfg(windows)]
fn device_info(letter: char) -> (Media, bool, Option<u32>) {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BusType1394, BusTypeSd, BusTypeUsb, CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::*;

    let path: Vec<u16> = format!(r"\\.\{letter}:").encode_utf16().chain(Some(0)).collect();
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return (Media::Unknown, false, None);
    }
    let query = |prop: STORAGE_PROPERTY_ID, out: &mut [u8]| -> bool {
        let q = STORAGE_PROPERTY_QUERY { PropertyId: prop, QueryType: PropertyStandardQuery, AdditionalParameters: [0] };
        let mut ret = 0u32;
        unsafe {
            DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &q as *const _ as *const _,
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                out.as_mut_ptr() as *mut _,
                out.len() as u32,
                &mut ret,
                std::ptr::null_mut(),
            ) != 0
        }
    };
    let mut seek = [0u8; std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>()];
    let media = if query(StorageDeviceSeekPenaltyProperty, &mut seek) {
        let d: DEVICE_SEEK_PENALTY_DESCRIPTOR = unsafe { std::ptr::read_unaligned(seek.as_ptr() as *const _) };
        if d.IncursSeekPenalty { Media::Hdd } else { Media::Ssd }
    } else {
        Media::Unknown
    };
    let mut dev = [0u8; 1024];
    let usb = query(StorageDeviceProperty, &mut dev) && {
        let d: STORAGE_DEVICE_DESCRIPTOR = unsafe { std::ptr::read_unaligned(dev.as_ptr() as *const _) };
        d.BusType == BusTypeUsb || d.BusType == BusTypeSd || d.BusType == BusType1394
    };
    let mut num = STORAGE_DEVICE_NUMBER { DeviceType: 0, DeviceNumber: 0, PartitionNumber: 0 };
    let mut ret = 0u32;
    let disk = unsafe {
        DeviceIoControl(
            h,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            std::ptr::null(),
            0,
            &mut num as *mut _ as *mut _,
            std::mem::size_of::<STORAGE_DEVICE_NUMBER>() as u32,
            &mut ret,
            std::ptr::null_mut(),
        ) != 0
    }
    .then_some(num.DeviceNumber);
    unsafe { CloseHandle(h) };
    (media, usb, disk)
}

#[cfg(not(windows))]
pub fn list_volumes() -> Vec<Volume> {
    Vec::new()
}

/// Concurrency for the directory walker, per impact profile and media.
/// HDDs get few readers because parallel reads turn into head seeks.
pub fn walker_threads(profile: crate::Profile, media: Media) -> usize {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    match (profile, media) {
        (crate::Profile::Low, _) => 1,
        (crate::Profile::Balanced, Media::Hdd) => 2,
        (crate::Profile::Balanced, _) => 4.min(cpus),
        (crate::Profile::Max, Media::Hdd) => 3,
        (crate::Profile::Max, _) => cpus.clamp(2, 16),
    }
}

/// Concurrent file readers for hashing / verification.
pub fn reader_threads(profile: crate::Profile, media: Media) -> usize {
    match (profile, media) {
        (_, Media::Hdd) => 1,
        (crate::Profile::Low, _) => 1,
        (crate::Profile::Balanced, _) => 3,
        (crate::Profile::Max, _) => 6,
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn finds_system_volume() {
        let vols = super::list_volumes();
        let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let v = vols.iter().find(|v| v.letter.eq_ignore_ascii_case(&sys)).expect("system drive listed");
        assert!(v.total_bytes > 0 && v.total_bytes >= v.free_bytes);
    }
}
