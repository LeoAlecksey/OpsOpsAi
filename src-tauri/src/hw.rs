//! What this computer has: memory, free disk where models go, and whether the engine can use the GPU.

use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub os: &'static str,
    pub arch: &'static str,
    /// total memory, bytes
    pub ram_total: u64,
    /// free disk where models are stored, bytes; None — unknown
    pub disk_free: Option<u64>,
    pub cpu_threads: usize,
    /// GPU acceleration is possible: Metal on Apple Silicon, a Vulkan driver on Windows/Linux
    pub gpu: bool,
    /// human name of the acceleration, for the UI
    pub gpu_name: String,
}

/// The Vulkan loader the GPU build of the engine links against (installed with the video driver).
pub fn vulkan_found() -> bool {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        Path::new(&root).join("System32").join("vulkan-1.dll").exists()
    }
    #[cfg(target_os = "linux")]
    {
        ["/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/usr/lib64", "/usr/lib", "/lib/x86_64-linux-gnu", "/lib/aarch64-linux-gnu", "/lib64"]
            .iter()
            .any(|d| Path::new(d).join("libvulkan.so.1").exists())
    }
    #[cfg(target_os = "macos")]
    {
        false
    }
}

/// Free space on the disk that holds `dir` (the longest mount point that contains it).
pub fn disk_free(dir: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let target = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    disks
        .list()
        .iter()
        .filter(|d| target.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

pub fn detect(models_dir: &Path) -> Hardware {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let mac_arm = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    let vulkan = !mac_arm && vulkan_found();
    Hardware {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        ram_total: sys.total_memory(),
        disk_free: disk_free(models_dir),
        cpu_threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
        gpu: mac_arm || vulkan,
        gpu_name: if mac_arm { "Apple Silicon (Metal)".into() } else if vulkan { "видеокарта (Vulkan)".into() } else { String::new() },
    }
}
