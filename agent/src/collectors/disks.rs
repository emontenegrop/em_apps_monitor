use std::{collections::HashSet, path::Path};

use em_protocol::metrics::DiskUsage;
use sysinfo::Disks;

/// Sistemas de archivos que no representan almacenamiento real
const PSEUDO_FS: &[&str] = &[
    "overlay", "tmpfs", "devtmpfs", "squashfs", "proc", "sysfs", "cgroup", "cgroup2",
    "nsfs", "fuse.lxcfs", "autofs", "devpts", "mqueue", "debugfs", "tracefs", "securityfs",
    "pstore", "bpf", "configfs", "hugetlbfs", "ramfs", "efivarfs", "binfmt_misc", "fusectl",
    "iso9660",
];

fn is_pseudo(fs: &str, mount: &str) -> bool {
    PSEUDO_FS.contains(&fs)
        || mount.starts_with("/proc")
        || mount.starts_with("/sys")
        || mount.starts_with("/dev")
        || mount.starts_with("/run")
        || mount.contains("/containers/storage/")
        || mount.contains("/docker/")
}

fn usage(name: String, mount_point: String, fs_type: String, total: u64, available: u64) -> DiskUsage {
    let used = total.saturating_sub(available);
    DiskUsage {
        name,
        mount_point,
        total_bytes: total,
        used_bytes: used,
        available_bytes: available,
        usage_percent: super::percent(used as f64, total as f64),
        fs_type,
    }
}

/// Discos del host. Con `host_root` (modo contenedor) lee los montajes del PID 1
/// del host, porque sysinfo dentro de un contenedor solo ve los del contenedor.
pub fn collect(host_root: Option<&Path>) -> Vec<DiskUsage> {
    match host_root {
        Some(root) => collect_from_host_root(root),
        None => collect_native(),
    }
}

fn collect_native() -> Vec<DiskUsage> {
    let mut seen = HashSet::new();
    Disks::new_with_refreshed_list()
        .iter()
        .filter_map(|d| {
            let fs = d.file_system().to_string_lossy().to_string();
            let mount = d.mount_point().display().to_string();
            let name = d.name().to_string_lossy().to_string();
            // El mismo dispositivo puede estar montado varias veces (bind mounts)
            if is_pseudo(&fs, &mount) || d.total_space() == 0 || !seen.insert(name.clone()) {
                return None;
            }
            Some(usage(name, mount, fs, d.total_space(), d.available_space()))
        })
        .collect()
}

#[cfg(unix)]
fn collect_from_host_root(root: &Path) -> Vec<DiskUsage> {
    let mounts = match std::fs::read_to_string(root.join("proc/1/mounts")) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("no se pudo leer {}/proc/1/mounts: {e}", root.display());
            return Vec::new();
        }
    };

    let mut seen = HashSet::new();
    mounts
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (device, mount, fs) = (parts.next()?, parts.next()?, parts.next()?);
            // /proc/mounts escapa los espacios como \040
            let mount = mount.replace("\\040", " ");
            if is_pseudo(fs, &mount) || !device.starts_with('/') || !seen.insert(device.to_string()) {
                return None;
            }
            let path = root.join(mount.trim_start_matches('/'));
            let (total, available) = statvfs(&path)?;
            (total > 0).then(|| usage(device.to_string(), mount, fs.to_string(), total, available))
        })
        .collect()
}

#[cfg(not(unix))]
fn collect_from_host_root(_root: &Path) -> Vec<DiskUsage> {
    tracing::warn!("host_root solo está soportado en Linux");
    Vec::new()
}

#[cfg(unix)]
fn statvfs(path: &Path) -> Option<(u64, u64)> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: c_path es una cadena C válida y st un buffer del tamaño correcto
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let frsize = st.f_frsize as u64;
    Some((st.f_blocks as u64 * frsize, st.f_bavail as u64 * frsize))
}

#[cfg(test)]
mod tests {
    use super::is_pseudo;

    #[test]
    fn skips_pseudo_filesystems() {
        assert!(is_pseudo("overlay", "/"));
        assert!(is_pseudo("ext4", "/var/lib/docker/overlay2/x/merged"));
        assert!(is_pseudo("xfs", "/run/media"));
        assert!(!is_pseudo("ext4", "/"));
        assert!(!is_pseudo("xfs", "/data"));
    }
}
