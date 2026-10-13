//! `em-agent check`: diagnóstico de la instalación en el host.

use em_protocol::messages::RuntimeKind;
use sysinfo::System;

use crate::{collectors::runtime, config::AgentConfig};

/// Imprime el diagnóstico y devuelve `true` si el agente puede trabajar
pub async fn run(cfg: &AgentConfig) -> bool {
    let mut ok = true;

    println!("Host");
    println!("  nombre:   {}", System::host_name().unwrap_or_else(|| "?".into()));
    println!("  sistema:  {}", System::long_os_version().unwrap_or_else(|| "?".into()));
    println!("  kernel:   {}", System::kernel_version().unwrap_or_else(|| "?".into()));
    println!("  arquit.:  {}", std::env::consts::ARCH);
    if let Some(root) = &cfg.host_root {
        let mounts = root.join("proc/1/mounts");
        let found = mounts.exists();
        ok &= found;
        println!("  host_root: {} ({})", root.display(), if found { "ok" } else { "falta proc/1/mounts" });
    }

    println!("\nRuntimes");
    let (clients, errors) = runtime::discover(&cfg.runtimes).await;
    for c in &clients {
        println!(
            "  ✔ {:?} {} en {}{}",
            c.kind,
            c.version.as_deref().unwrap_or("?"),
            c.socket,
            if c.rootless { " (rootless)" } else { "" }
        );
        if c.kind == RuntimeKind::Podman && c.rootless {
            if let Some(msg) = rootless_cpu_delegation_problem() {
                println!("    ⚠ {msg}");
            }
        }
    }
    for (socket, err) in &errors {
        println!("  ✘ {socket}: {err:#}");
        if let Some(hint) = hint_for(err) {
            println!("    → {hint}");
        }
    }
    ok &= !clients.is_empty();

    println!("\nCentral");
    match &cfg.server_url {
        Some(url) => println!("  url: {url}"),
        None => println!("  url: (sin configurar — se define al inscribir el agente)"),
    }
    println!(
        "  CA:  {} ({})",
        cfg.ca_file.display(),
        if cfg.ca_file.exists() { "ok" } else { "no existe" }
    );

    println!("\nResultado: {}", if ok { "OK" } else { "con problemas" });
    ok
}

fn hint_for(err: &anyhow::Error) -> Option<&'static str> {
    let text = format!("{err:#}");
    if text.contains("Permission denied") || text.contains("os error 13") {
        Some("sin permiso sobre el socket: agrega el usuario del agente al grupo `docker`, o da acceso al socket de Podman con una ACL")
    } else if text.contains("No such file") || text.contains("os error 2") {
        Some("el socket no existe: para Podman ejecuta `systemctl enable --now podman.socket` (o `--user` en modo rootless)")
    } else {
        None
    }
}

/// En Podman rootless con cgroups v2, la CPU por contenedor requiere delegar el controlador `cpu`
#[cfg(unix)]
fn rootless_cpu_delegation_problem() -> Option<String> {
    // SAFETY: getuid no tiene precondiciones
    let uid = unsafe { libc::getuid() };
    let path = format!("/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service/cgroup.controllers");
    match std::fs::read_to_string(&path) {
        Ok(c) if c.split_whitespace().any(|x| x == "cpu") => None,
        Ok(_) => Some(format!(
            "el controlador `cpu` no está delegado al usuario {uid}: la CPU de los contenedores saldrá vacía. \
             Crea /etc/systemd/system/user@.service.d/delegate.conf con `[Service]\\nDelegate=cpu cpuset io memory pids` y reinicia la sesión"
        )),
        Err(_) => Some(format!("no se pudo leer {path} (¿cgroups v1?); las stats rootless pueden estar incompletas")),
    }
}

#[cfg(not(unix))]
fn rootless_cpu_delegation_problem() -> Option<String> {
    None
}
