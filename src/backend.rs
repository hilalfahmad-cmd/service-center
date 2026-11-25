use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub const APP_ID: &str = "io.github.hilalfajar.ServiceCenter";
pub const UNIT_PREFIX: &str = "sc-";

pub fn data_home() -> PathBuf {
    if let Ok(v) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(v)
    } else {
        dirs::home_dir().unwrap_or_default().join(".local/share")
    }
}

pub fn config_home() -> PathBuf {
    if let Ok(v) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(v)
    } else {
        dirs::home_dir().unwrap_or_default().join(".config")
    }
}

pub fn registry_file() -> PathBuf {
    data_home().join("service-center").join("services.json")
}

pub fn unit_dir() -> PathBuf {
    config_home().join("systemd").join("user")
}

pub const RESTART_OPTIONS: [&str; 3] = ["on-failure", "always", "no"];
pub fn restart_labels(key: &str) -> String {
    match key {
        "on-failure" => "Saat gagal".into(),
        "always" => "Selalu".into(),
        "no" => "Tidak".into(),
        _ => key.to_string(),
    }
}

static SAFE_ARG: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[A-Za-z0-9_@%+=:,./-]+$").unwrap());

fn _run(cmd: &[&str], timeout_secs: u64) -> Result<std::process::Output, String> {
    let mut c = Command::new(cmd[0]);
    for a in &cmd[1..] {
        c.arg(a);
    }
    // timeout not enforced simply; use child wait with timeout via thread is complex; keep simple
    c.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("Perintah '{}' tidak ditemukan", cmd[0])
        } else {
            format!("Gagal menjalankan '{}': {}", cmd[0], e)
        }
    })
}

pub fn systemctl(args: &[&str], timeout: u64) -> Result<std::process::Output, String> {
    let mut cmd = vec!["systemctl", "--user"];
    cmd.extend_from_slice(args);
    _run(&cmd, timeout)
}

pub fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| {
        dirs::home_dir()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default()
    })
}

pub fn linger_enabled() -> bool {
    match _run(
        &["loginctl", "show-user", &current_user(), "-p", "Linger"],
        10,
    ) {
        Ok(out) => {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim().ends_with("yes")
        }
        Err(_) => false,
    }
}

pub fn enable_linger() -> (bool, String) {
    match _run(
        &["pkexec", "loginctl", "enable-linger", &current_user()],
        120,
    ) {
        Ok(out) => {
            if out.status.success() {
                (true, String::new())
            } else {
                let msg = String::from_utf8_lossy(&out.stderr);
                let msg = if msg.trim().is_empty() {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    msg.trim().to_string()
                };
                (
                    false,
                    if msg.is_empty() {
                        "Gagal mengaktifkan linger".into()
                    } else {
                        msg
                    },
                )
            }
        }
        Err(e) => (false, e),
    }
}

pub fn slugify(name: &str) -> String {
    let lower = name.to_lowercase();
    let re = regex::Regex::new(r"[^a-z0-9]+").unwrap();
    let s = re.replace_all(&lower, "-");
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "layanan".into()
    } else {
        s
    }
}

pub fn pretty_name(path: &Path) -> String {
    let base = if path.is_file() {
        path.parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let base = base.replace(['-', '_'], " ");
    let base = base.trim().to_string();
    if base.is_empty() {
        "Layanan".into()
    } else {
        // title case simple
        base.split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn quote_unit_arg(value: &str) -> String {
    if !value.is_empty() && SAFE_ARG.is_match(value) {
        return value.to_string();
    }
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn quote_env(key: &str, value: &str) -> String {
    let text = format!("{}={}", key, value).replace('%', "%%");
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn is_elf(path: &Path) -> bool {
    if let Ok(mut f) = fs::File::open(path) {
        let mut buf = [0u8; 4];
        if f.read_exact(&mut buf).is_ok() {
            return buf == [0x7f, b'E', b'L', b'F'];
        }
    }
    false
}

fn python_for(path: &Path) -> String {
    for base in [path.parent(), path.parent().and_then(|p| p.parent())] {
        if let Some(base) = base {
            for venv in [".venv", "venv", "env"] {
                let cand = base.join(venv).join("bin").join("python");
                if cand.is_file() && is_elf(&cand) {
                    return cand.to_string_lossy().to_string();
                }
            }
        }
    }
    // shutil.which python3
    if let Ok(out) = Command::new("which").arg("python3").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    "/usr/bin/python3".into()
}

pub fn detect_command(path: &Path) -> Result<(Vec<String>, PathBuf), String> {
    let path = shellexpand::tilde(&path.to_string_lossy()).to_string();
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err("File tidak ditemukan".into());
    }
    let workdir = if path.is_file() {
        path.parent().unwrap_or(&path).to_path_buf()
    } else {
        path.clone()
    };

    if path.file_name().and_then(|n| n.to_str()) == Some("package.json") {
        let content =
            fs::read_to_string(&path).map_err(|_| "package.json tidak bisa dibaca".to_string())?;
        let pkg: serde_json::Value = serde_json::from_str(&content)
            .map_err(|_| "package.json tidak bisa dibaca".to_string())?;
        let scripts = pkg.get("scripts").and_then(|v| v.as_object());
        if let Some(start) = scripts
            .and_then(|s| s.get("start"))
            .and_then(|v| v.as_str())
        {
            let npm = which("npm").unwrap_or_else(|| "npm".into());
            return Ok((vec![npm, "start".into()], workdir));
        }
        if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
            let mainpath = workdir.join(main);
            if mainpath.is_file() {
                let node = which("node").unwrap_or_else(|| "/usr/bin/node".into());
                return Ok((vec![node, mainpath.to_string_lossy().to_string()], workdir));
            }
        }
        let indexjs = workdir.join("index.js");
        if indexjs.is_file() {
            let node = which("node").unwrap_or_else(|| "/usr/bin/node".into());
            return Ok((vec![node, indexjs.to_string_lossy().to_string()], workdir));
        }
        return Err("package.json tidak punya scripts.start atau main. Isi perintah manual".into());
    }

    let suffix = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    match suffix.as_str() {
        "js" | "mjs" | "cjs" => {
            let node = which("node").unwrap_or_else(|| "/usr/bin/node".into());
            Ok((vec![node, path.to_string_lossy().to_string()], workdir))
        }
        "py" => Ok((
            vec![python_for(&path), path.to_string_lossy().to_string()],
            workdir,
        )),
        "sh" | "bash" => Ok((
            vec!["bash".into(), path.to_string_lossy().to_string()],
            workdir,
        )),
        "jar" => {
            let java = which("java").unwrap_or_else(|| "java".into());
            Ok((
                vec![java, "-jar".into(), path.to_string_lossy().to_string()],
                workdir,
            ))
        }
        _ => {
            // executable file
            if path.is_file() {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = fs::metadata(&path) {
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Ok((vec![path.to_string_lossy().to_string()], workdir));
                    }
                }
            }
            Err(
                "Tipe file tidak dikenali. Pilih .py, .js, .sh, .jar, package.json, atau binary"
                    .into(),
            )
        }
    }
}

fn which(bin: &str) -> Option<String> {
    Command::new("which")
        .arg(bin)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub file: String,
    pub command: Vec<String>,
    pub workdir: String,
    pub env: BTreeMap<String, String>,
    pub restart: String,
    pub created_at: f64,
}

impl Service {
    pub fn unit_name(&self) -> String {
        format!("{}{}.service", UNIT_PREFIX, self.id)
    }

    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "file": self.file,
            "command": self.command,
            "workdir": self.workdir,
            "env": self.env,
            "restart": self.restart,
            "created_at": self.created_at,
        })
    }

    pub fn from_dict(id: &str, data: &serde_json::Value) -> Result<Self, String> {
        let name = data["name"].as_str().ok_or("missing name")?.to_string();
        let file = data["file"].as_str().ok_or("missing file")?.to_string();
        let command: Vec<String> = data["command"]
            .as_array()
            .ok_or("missing command")?
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();
        let workdir = data["workdir"]
            .as_str()
            .ok_or("missing workdir")?
            .to_string();
        let env: BTreeMap<String, String> = data
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let restart = data
            .get("restart")
            .and_then(|v| v.as_str())
            .unwrap_or("on-failure")
            .to_string();
        let created_at = data
            .get("created_at")
            .and_then(|v| v.as_f64())
            .unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64()
            });
        Ok(Service {
            id: id.to_string(),
            name,
            file,
            command,
            workdir,
            env,
            restart,
            created_at,
        })
    }
}

pub fn build_unit(service: &Service) -> String {
    let exec_start = service
        .command
        .iter()
        .map(|p| quote_unit_arg(p))
        .collect::<Vec<_>>()
        .join(" ");
    let restart = if RESTART_OPTIONS.contains(&service.restart.as_str()) {
        service.restart.clone()
    } else {
        "on-failure".into()
    };
    let mut lines: Vec<String> = vec![];
    lines.push("[Unit]".into());
    lines.push(format!("Description={}", service.name.replace('%', "%%")));
    lines.push(format!(
        "ConditionPathExists={}",
        quote_unit_arg(&service.file)
    ));
    lines.push("After=network.target".into());
    lines.push(String::new());
    lines.push("[Service]".into());
    lines.push("Type=simple".into());
    lines.push(format!(
        "WorkingDirectory={}",
        quote_unit_arg(&service.workdir)
    ));
    lines.push(format!("ExecStart={}", exec_start));
    lines.push(format!("Restart={}", restart));
    lines.push("RestartSec=3".into());
    lines.push("SyslogIdentifier=service-center".into());
    for (key, value) in &service.env {
        let key = key.trim();
        if !key.is_empty() {
            lines.push(format!("Environment={}", quote_env(key, value)));
        }
    }
    lines.push(String::new());
    lines.push("[Install]".into());
    lines.push("WantedBy=default.target".into());
    lines.push(String::new());
    lines.join("\n")
}

#[derive(Debug, Clone, Default)]
pub struct ServiceState {
    pub active: String,
    pub sub: String,
    pub enabled: String,
    pub pid: String,
    pub description: String,
    pub fragment_path: String,
    pub exec_start: String,
}

impl ServiceState {
    pub fn running(&self) -> bool {
        self.active == "active"
    }
    pub fn failed(&self) -> bool {
        self.active == "failed"
    }
}

pub fn parse_show(output: &str) -> BTreeMap<String, ServiceState> {
    let mut states: BTreeMap<String, ServiceState> = BTreeMap::new();
    let mut block: BTreeMap<String, String> = BTreeMap::new();

    let flush = |block: &BTreeMap<String, String>, states: &mut BTreeMap<String, ServiceState>| {
        if let Some(unit) = block.get("Id") {
            states.insert(
                unit.clone(),
                ServiceState {
                    active: block
                        .get("ActiveState")
                        .cloned()
                        .unwrap_or_else(|| "inactive".into()),
                    sub: block
                        .get("SubState")
                        .cloned()
                        .unwrap_or_else(|| "dead".into()),
                    enabled: block.get("UnitFileState").cloned().unwrap_or_default(),
                    pid: block.get("MainPID").cloned().unwrap_or_default(),
                    description: block.get("Description").cloned().unwrap_or_default(),
                    fragment_path: block.get("FragmentPath").cloned().unwrap_or_default(),
                    exec_start: block.get("ExecStart").cloned().unwrap_or_default(),
                },
            );
        }
    };

    for line in output.lines() {
        if line.trim().is_empty() {
            flush(&block, &mut states);
            block.clear();
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let k = parts.next().unwrap_or("");
        let v = parts.next().unwrap_or("");
        if !k.is_empty() && line.contains('=') {
            block.insert(k.to_string(), v.to_string());
        }
    }
    flush(&block, &mut states);
    states
}

// helper trait for partition
trait PartitionExt {
    fn partition(&self, sep: char) -> (&str, &str, &str);
}
impl PartitionExt for str {
    fn partition(&self, sep: char) -> (&str, &str, &str) {
        if let Some(i) = self.find(sep) {
            (
                &self[..i],
                &self[i..i + sep.len_utf8()],
                &self[i + sep.len_utf8()..],
            )
        } else {
            (self, "", "")
        }
    }
}

pub struct Manager {
    pub registry_file: PathBuf,
    pub unit_dir: PathBuf,
    services: BTreeMap<String, Service>,
}

impl Manager {
    pub fn new() -> Self {
        let rf = registry_file();
        let ud = unit_dir();
        let mut m = Manager {
            registry_file: rf.clone(),
            unit_dir: ud.clone(),
            services: BTreeMap::new(),
        };
        m.services = m.load();
        m
    }

    fn load(&self) -> BTreeMap<String, Service> {
        let mut services = BTreeMap::new();
        if let Ok(raw) = fs::read_to_string(&self.registry_file) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(obj) = val.get("services").and_then(|v| v.as_object()) {
                    for (sid, data) in obj {
                        if let Ok(svc) = Service::from_dict(sid, data) {
                            services.insert(sid.clone(), svc);
                        }
                    }
                }
            }
        }
        services
    }

    pub fn reload(&mut self) {
        self.services = self.load();
    }

    pub fn save(&self) -> Result<(), String> {
        self.registry_file
            .parent()
            .map(|p| fs::create_dir_all(p))
            .transpose()
            .map_err(|e| e.to_string())?;
        let payload = serde_json::json!({
            "version": 1,
            "services": self.services.values().map(|s| (s.id.clone(), s.to_dict())).collect::<serde_json::Map<_,_>>(),
        });
        // build map properly
        let mut map = serde_json::Map::new();
        for s in self.services.values() {
            map.insert(s.id.clone(), s.to_dict());
        }
        let payload = serde_json::json!({"version":1,"services": map});
        fs::write(
            &self.registry_file,
            serde_json::to_string_pretty(&payload).unwrap() + "\n",
        )
        .map_err(|e| e.to_string())
    }

    pub fn services(&self) -> Vec<Service> {
        let mut v: Vec<Service> = self.services.values().cloned().collect();
        v.sort_by_key(|s| s.name.to_lowercase());
        v
    }

    pub fn get(&self, sid: &str) -> Option<Service> {
        self.services.get(sid).cloned()
    }

    pub fn unit_path(&self, sid: &str) -> PathBuf {
        self.unit_dir
            .join(format!("{}{}.service", UNIT_PREFIX, sid))
    }

    pub fn unit_exists(&self, sid: &str) -> bool {
        self.unit_path(sid).exists()
    }

    fn unique_id(&self, base: &str) -> String {
        let mut candidate = base.to_string();
        let mut counter = 2;
        while self.services.contains_key(&candidate) || self.unit_path(&candidate).exists() {
            candidate = format!("{}-{}", base, counter);
            counter += 1;
        }
        candidate
    }

    pub fn add(
        &mut self,
        name: &str,
        file: &str,
        command: Vec<String>,
        workdir: &str,
        env: BTreeMap<String, String>,
        restart: &str,
        autostart: bool,
        start_now: bool,
    ) -> Result<(Service, Vec<String>), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Nama layanan wajib diisi".into());
        }
        let file_path = PathBuf::from(shellexpand::tilde(file).to_string());
        if !file_path.exists() {
            return Err("File program tidak ditemukan".into());
        }
        let workdir_path = PathBuf::from(shellexpand::tilde(workdir).to_string());
        if !workdir_path.is_dir() {
            return Err("Direktori kerja tidak ditemukan".into());
        }
        if command.is_empty() {
            return Err("Perintah kosong".into());
        }
        let restart = if RESTART_OPTIONS.contains(&restart) {
            restart.to_string()
        } else {
            "on-failure".into()
        };

        let sid = self.unique_id(&slugify(name));
        let service = Service {
            id: sid.clone(),
            name: name.to_string(),
            file: file_path.to_string_lossy().to_string(),
            command: command.clone(),
            workdir: workdir_path.to_string_lossy().to_string(),
            env: env.clone(),
            restart: restart.clone(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
        };

        fs::create_dir_all(&self.unit_dir).map_err(|e| e.to_string())?;
        fs::write(self.unit_path(&sid), build_unit(&service)).map_err(|e| e.to_string())?;
        self.services.insert(sid.clone(), service.clone());
        self.save()?;

        let mut warnings: Vec<String> = vec![];
        // daemon-reload
        let reload = systemctl(&["daemon-reload"], 30);
        match reload {
            Ok(out) if !out.status.success() => {
                let msg = String::from_utf8_lossy(&out.stderr);
                let msg = if msg.trim().is_empty() {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    msg.trim().to_string()
                };
                warnings.push(if msg.is_empty() {
                    "daemon-reload gagal".into()
                } else {
                    msg
                });
            }
            Err(e) => warnings.push(e),
            _ => {}
        }
        if autostart {
            if let Ok((ok, msg)) = self.set_enabled(&sid, true) {
                if !ok {
                    warnings.push(format!("Autostart gagal: {}", msg));
                }
            }
        }
        if start_now {
            if let Ok((ok, msg)) = self.start(&sid) {
                if !ok {
                    warnings.push(format!("Gagal menjalankan: {}", msg));
                }
            }
        }
        Ok((service, warnings))
    }

    pub fn update(
        &mut self,
        sid: &str,
        name: &str,
        file: &str,
        command: Vec<String>,
        workdir: &str,
        env: BTreeMap<String, String>,
        restart: &str,
        autostart: bool,
        restart_now: bool,
    ) -> Result<(Service, Vec<String>), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Nama layanan wajib diisi".into());
        }
        let file_path = PathBuf::from(shellexpand::tilde(file).to_string());
        if !file_path.exists() {
            return Err("File program tidak ditemukan".into());
        }
        let workdir_path = PathBuf::from(shellexpand::tilde(workdir).to_string());
        if !workdir_path.is_dir() {
            return Err("Direktori kerja tidak ditemukan".into());
        }
        if command.is_empty() {
            return Err("Perintah kosong".into());
        }
        let restart = if RESTART_OPTIONS.contains(&restart) {
            restart.to_string()
        } else {
            "on-failure".into()
        };

        let svc = self
            .services
            .get(sid)
            .ok_or("Layanan tidak ditemukan")?
            .clone();
        let was_enabled = self
            .states(Some(vec![svc.unit_name()]))
            .get(&svc.unit_name())
            .map(|s| s.enabled == "enabled")
            .unwrap_or(false);
        let was_running = self
            .states(Some(vec![svc.unit_name()]))
            .get(&svc.unit_name())
            .map(|s| s.running())
            .unwrap_or(false);

        let updated = Service {
            id: sid.to_string(),
            name: name.to_string(),
            file: file_path.to_string_lossy().to_string(),
            command: command.clone(),
            workdir: workdir_path.to_string_lossy().to_string(),
            env: env.clone(),
            restart: restart.clone(),
            created_at: svc.created_at,
        };

        fs::create_dir_all(&self.unit_dir).map_err(|e| e.to_string())?;
        fs::write(self.unit_path(sid), build_unit(&updated)).map_err(|e| e.to_string())?;
        self.services.insert(sid.to_string(), updated.clone());
        self.save()?;

        let mut warnings: Vec<String> = vec![];
        match self.daemon_reload() {
            Ok(out) if !out.status.success() => {
                let msg = String::from_utf8_lossy(&out.stderr);
                let msg = if msg.trim().is_empty() {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    msg.trim().to_string()
                };
                warnings.push(if msg.is_empty() {
                    "daemon-reload gagal".into()
                } else {
                    msg
                });
            }
            Err(e) => warnings.push(e),
            _ => {}
        }
        // autostart
        if autostart != was_enabled {
            if let Ok((ok, msg)) = self.set_enabled(sid, autostart) {
                if !ok {
                    warnings.push(format!("Autostart gagal: {}", msg));
                }
            }
        }
        if restart_now {
            if was_running {
                if let Ok((ok, msg)) = self.restart(sid) {
                    if !ok {
                        warnings.push(format!("Restart gagal: {}", msg));
                    }
                }
            } else {
                // start if requested? keep off
            }
        }
        Ok((updated, warnings))
    }

    pub fn action(&self, sid: &str, verb: &str) -> Result<(bool, String), String> {
        if !["start", "stop", "restart"].contains(&verb) {
            return Err(format!("Aksi tidak dikenal: {}", verb));
        }
        let service = self.services.get(sid).ok_or("Layanan tidak ditemukan")?;
        let unit = service.unit_name();
        let out = systemctl(&[verb, &unit], 30).map_err(|e| e)?;
        if out.status.success() {
            Ok((true, String::new()))
        } else {
            let msg = String::from_utf8_lossy(&out.stderr);
            let msg = if msg.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                msg.trim().to_string()
            };
            Ok((
                false,
                if msg.is_empty() {
                    format!("{} gagal", verb)
                } else {
                    msg
                },
            ))
        }
    }

    pub fn start(&self, sid: &str) -> Result<(bool, String), String> {
        self.action(sid, "start")
    }
    pub fn stop(&self, sid: &str) -> Result<(bool, String), String> {
        self.action(sid, "stop")
    }
    pub fn restart(&self, sid: &str) -> Result<(bool, String), String> {
        self.action(sid, "restart")
    }

    pub fn set_enabled(&self, sid: &str, enabled: bool) -> Result<(bool, String), String> {
        let service = self.services.get(sid).ok_or("Layanan tidak ditemukan")?;
        let unit = service.unit_name();
        let verb = if enabled { "enable" } else { "disable" };
        let out = systemctl(&[verb, &unit], 30).map_err(|e| e)?;
        if out.status.success() {
            Ok((true, String::new()))
        } else {
            let msg = String::from_utf8_lossy(&out.stderr);
            let msg = if msg.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                msg.trim().to_string()
            };
            Ok((
                false,
                if msg.is_empty() {
                    format!("{} gagal", verb)
                } else {
                    msg
                },
            ))
        }
    }

    pub fn states(&self, units: Option<Vec<String>>) -> BTreeMap<String, ServiceState> {
        let units =
            units.unwrap_or_else(|| self.services().iter().map(|s| s.unit_name()).collect());
        if units.is_empty() {
            return BTreeMap::new();
        }
        let mut args: Vec<&str> = vec!["show"];
        let unit_refs: Vec<&str> = units.iter().map(|s| s.as_str()).collect();
        for u in &unit_refs {
            args.push(u);
        }
        args.push("-p");
        args.push("Id");
        args.push("-p");
        args.push("ActiveState");
        args.push("-p");
        args.push("SubState");
        args.push("-p");
        args.push("UnitFileState");
        args.push("-p");
        args.push("MainPID");
        args.push("-p");
        args.push("Description");
        args.push("-p");
        args.push("FragmentPath");
        args.push("-p");
        args.push("ExecStart");
        match systemctl(&args, 30) {
            Ok(out) if out.status.success() => parse_show(&String::from_utf8_lossy(&out.stdout)),
            _ => BTreeMap::new(),
        }
    }

    pub fn system_services(&self) -> Result<Vec<SystemService>, String> {
        let out = systemctl(
            &[
                "list-units",
                "--type=service",
                "--all",
                "--no-legend",
                "--plain",
                "--full",
                "--no-pager",
            ],
            30,
        )?;
        if !out.status.success() {
            let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if detail.is_empty() {
                "Gagal membaca daftar layanan systemd pengguna".into()
            } else {
                detail
            });
        }

        let registered: std::collections::HashSet<String> = self
            .services()
            .iter()
            .map(|service| service.unit_name())
            .collect();
        let mut services = filter_unregistered_system_services(
            &parse_unit_list(&String::from_utf8_lossy(&out.stdout)),
            &registered,
        );
        services.sort_by(|left, right| left.unit.cmp(&right.unit));
        Ok(services)
    }

    fn is_discovered_system_unit(&self, unit: &str) -> Result<bool, String> {
        Ok(self
            .system_services()?
            .iter()
            .any(|service| service.unit == unit))
    }

    pub fn system_action(&self, unit: &str, verb: &str) -> Result<(bool, String), String> {
        if !["start", "stop", "restart"].contains(&verb) {
            return Err(format!("Aksi tidak dikenal: {}", verb));
        }
        if !self.is_discovered_system_unit(unit)? {
            return Err("Layanan systemd tidak ditemukan".into());
        }
        let out = systemctl(&[verb, unit], 30)?;
        if out.status.success() {
            Ok((true, String::new()))
        } else {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let msg = if msg.is_empty() {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                msg
            };
            Ok((
                false,
                if msg.is_empty() {
                    format!("{} gagal", verb)
                } else {
                    msg
                },
            ))
        }
    }

    pub fn logs(&self, sid: &str, lines: usize) -> Result<(bool, String), String> {
        let service = self.services.get(sid).ok_or("Layanan tidak ditemukan")?;
        self.logs_for_unit(&service.unit_name(), lines)
    }

    pub fn system_logs(&self, unit: &str, lines: usize) -> Result<(bool, String), String> {
        if !self.is_discovered_system_unit(unit)? {
            return Err("Layanan systemd tidak ditemukan".into());
        }
        self.logs_for_unit(unit, lines)
    }

    fn logs_for_unit(&self, unit: &str, lines: usize) -> Result<(bool, String), String> {
        let n = lines.to_string();
        let out = _run(
            &[
                "journalctl",
                "--user",
                "-u",
                unit,
                "-n",
                &n,
                "--no-pager",
                "-o",
                "short-iso",
            ],
            20,
        )
        .map_err(|e| e)?;
        if out.status.success() {
            Ok((true, String::from_utf8_lossy(&out.stdout).to_string()))
        } else {
            let msg = String::from_utf8_lossy(&out.stderr);
            let msg = if msg.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                msg.trim().to_string()
            };
            Ok((
                false,
                if msg.is_empty() {
                    "journalctl gagal".into()
                } else {
                    msg
                },
            ))
        }
    }

    pub fn delete(&mut self, sid: &str) -> Result<(), String> {
        let service = self
            .services
            .get(sid)
            .ok_or("Layanan tidak ditemukan")?
            .clone();
        let unit = service.unit_name();
        let _ = systemctl(&["stop", &unit], 30);
        let _ = systemctl(&["disable", &unit], 30);
        let path = self.unit_path(sid);
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        self.services.remove(sid);
        self.save()?;
        let _ = systemctl(&["daemon-reload"], 30);
        let _ = systemctl(&["reset-failed", &unit], 30);
        Ok(())
    }

    pub fn daemon_reload(&self) -> Result<std::process::Output, String> {
        systemctl(&["daemon-reload"], 30)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemService {
    pub unit: String,
    pub description: String,
}

pub fn parse_unit_list(output: &str) -> Vec<SystemService> {
    output
        .lines()
        .filter_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            if columns.len() < 4 || !columns[0].ends_with(".service") {
                return None;
            }
            let unit = columns[0];
            let description = columns[4..].join(" ");
            Some(SystemService {
                unit: unit.to_string(),
                description: if description.is_empty() {
                    unit.trim_end_matches(".service").replace('-', " ")
                } else {
                    description
                },
            })
        })
        .collect()
}

fn filter_unregistered_system_services(
    services: &[SystemService],
    registered_units: &std::collections::HashSet<String>,
) -> Vec<SystemService> {
    services
        .iter()
        .filter(|service| !registered_units.contains(&service.unit))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_systemd_service_rows_and_ignores_other_units() {
        let services = parse_unit_list(
            "dbus.service loaded active running D-Bus User Message Bus\n\
             app.socket loaded active listening App socket\n\
             worker.service loaded inactive dead\n\
             malformed\n",
        );

        assert_eq!(
            services,
            vec![
                SystemService {
                    unit: "dbus.service".into(),
                    description: "D-Bus User Message Bus".into(),
                },
                SystemService {
                    unit: "worker.service".into(),
                    description: "worker".into(),
                },
            ]
        );
    }

    #[test]
    fn parse_show_includes_unit_details() {
        let states = parse_show(
            "Id=worker.service\nActiveState=active\nSubState=running\n\
             Description=Background worker\nFragmentPath=/usr/lib/systemd/user/worker.service\n\
             ExecStart={ path=/usr/bin/worker ; argv[]=/usr/bin/worker --serve ; }",
        );
        let state = states.get("worker.service").unwrap();

        assert_eq!(state.description, "Background worker");
        assert_eq!(state.fragment_path, "/usr/lib/systemd/user/worker.service");
        assert!(state.exec_start.contains("/usr/bin/worker --serve"));
    }

    #[test]
    fn excludes_manually_registered_units_from_systemd_discovery() {
        let services = vec![
            SystemService {
                unit: "sc-api.service".into(),
                description: "API".into(),
            },
            SystemService {
                unit: "dbus.service".into(),
                description: "D-Bus".into(),
            },
        ];
        let registered = ["sc-api.service".to_string()].into_iter().collect();

        assert_eq!(
            filter_unregistered_system_services(&services, &registered),
            vec![services[1].clone()]
        );
    }
}
