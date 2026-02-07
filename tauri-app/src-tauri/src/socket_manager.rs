use std::path::{Path, PathBuf};

pub struct SocketManager {
    instance_id: u32,
    created_sockets: Vec<PathBuf>,
}

impl SocketManager {
    pub fn new() -> Self {
        Self {
            instance_id: std::process::id(),
            created_sockets: Vec::new(),
        }
    }

    pub fn socket_path(&self, terminal_id: &str) -> PathBuf {
        PathBuf::from(format!(
            "/tmp/libg-nvim-{}-{}.sock",
            self.instance_id, terminal_id
        ))
    }

    pub fn register(&mut self, path: PathBuf) {
        if !self.created_sockets.contains(&path) {
            self.created_sockets.push(path);
        }
    }

    pub fn remove_socket(&mut self, path: &Path) {
        let _ = std::fs::remove_file(path);
        self.created_sockets.retain(|p| p != path);
    }

    pub fn cleanup_all(&mut self) {
        for path in self.created_sockets.drain(..) {
            let _ = std::fs::remove_file(&path);
        }
    }

    /// Remove sockets left behind by dead processes.
    /// Scans /tmp for `libg-nvim-{pid}-*.sock` and removes any whose PID is no longer alive.
    pub fn cleanup_stale() {
        let Ok(entries) = std::fs::read_dir("/tmp") else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with("libg-nvim-") || !name.ends_with(".sock") {
                continue;
            }
            // Extract PID: "libg-nvim-{pid}-{terminalId}.sock"
            let inner = &name["libg-nvim-".len()..name.len() - ".sock".len()];
            let Some(pid_str) = inner.split('-').next() else {
                continue;
            };
            let Ok(pid) = pid_str.parse::<i32>() else {
                continue;
            };
            // Check if the process is still alive
            let alive = unsafe { libc::kill(pid, 0) } == 0;
            if !alive {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

impl Drop for SocketManager {
    fn drop(&mut self) {
        self.cleanup_all();
    }
}
