use anyhow::{Context, Result, anyhow, bail};
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Log = Vec<String>;

pub const APP_ID: &str = "orangsign";
pub const CERT_FILE: &str = "cert.pfx";
// this was generated using OrangCalc, my flex
pub const CERT_PASSWORD: &str = "6B2PA-Dhf!UPyYA$kPgc5+R0vFY041-sAZ>Zu^]m<5v8=+VRZ_Ize}AMfaDp4>AW";
pub const CERT_SUBJECT: &str = "/CN=OrangStudio";
pub const TIMESTAMP_URL: &str = "http://timestamp.digicert.com";
pub const CONTEXT_MENU_TEXT: &str = "Sign this file with OrangSign cert";

const PASS_ENV: &str = "ORANGSIGN_PASS";
const LEGACY_INSTALL_BIN: &str = "/usr/local/bin/orangsign";
const LEGACY_INSTALL_DESKTOP: &str = "/usr/local/share/applications/orangsign.desktop";
const LEGACY_INSTALL_ICON: &str = "/usr/local/share/icons/hicolor/256x256/apps/orangsign.png";
const LEGACY_INSTALL_PIXMAP: &str = "/usr/share/pixmaps/orangsign.png";
const LEGACY_INSTALL_SVG: &str = "/usr/local/share/icons/hicolor/scalable/apps/orangsign.svg";

pub const LOGO_PNG: &[u8] = include_bytes!("../orangeS.png");

const TRUST_STORES: &[(&str, &str, &[&str])] = &[
    (
        "/etc/ca-certificates/trust-source/anchors",
        "update-ca-trust",
        &["extract"],
    ),
    (
        "/etc/pki/ca-trust/source/anchors",
        "update-ca-trust",
        &["extract"],
    ),
    (
        "/usr/local/share/ca-certificates",
        "update-ca-certificates",
        &[],
    ),
    ("/etc/pki/trust/anchors", "update-ca-certificates", &[]),
];
const TRUST_FILE: &str = "orangsign.crt";

pub fn current_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from(APP_ID))
}

fn exe_dir() -> PathBuf {
    current_exe()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg_data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
}

pub fn data_dir() -> PathBuf {xdg_data_home().join("OrangStudio").join("OrangSign")}

pub fn cert_path() -> PathBuf {
    let portable = exe_dir().join(CERT_FILE);
    if portable.is_file() {
        portable
    } else {data_dir().join(CERT_FILE)}
}

pub fn installed_exe() -> PathBuf {data_dir().join(APP_ID)}

pub fn is_installed() -> bool {installed_exe().is_file()}

pub fn which(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .chain(["/usr/sbin", "/sbin", "/usr/bin"].map(PathBuf::from))
        .map(|d| d.join(tool))
        .find(|p| p.is_file())
}

fn run(cmd: &mut Command) -> Result<String> {
    let prog = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd
        .output()
        .with_context(|| format!("failed to start `{prog}` - is it installed?"))?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    if out.status.success() {return Ok(stdout);}

    let mut msg = format!("`{prog}` failed ({})", out.status);
    if !stdout.is_empty() || !stderr.is_empty() {
        msg.push_str(":\n");
        if !stdout.is_empty() {
            msg.push_str(&stdout);
            if !stderr.is_empty() {
                msg.push('\n');
            }
        }
        msg.push_str(&stderr);
    }
    Err(anyhow!(msg))
}

fn openssl() -> Command {
    let mut cmd = Command::new("openssl");
    cmd.env(PASS_ENV, CERT_PASSWORD);
    cmd
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("orangsign-{}-{nanos}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .with_context(|| format!("cannot create {}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {let _ = fs::remove_dir_all(&self.0);}
}

fn write_file(path: &Path, contents: &str, mode: u32) -> Result<()> {
    write_bytes(path, contents.as_bytes(), mode)
}

fn write_bytes(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

pub fn ensure_user_icon() {
    let data = xdg_data_home();
    let _ = write_bytes(
        &data.join("icons/hicolor/256x256/apps/orangsign.png"),
        LOGO_PNG,
        0o644,
    );

    let user_desktop = data.join("applications/orangsign.desktop");
    if is_installed() || !user_desktop.exists() {
        let exe = if is_installed() {installed_exe()} else {current_exe()};
        let exe_str = exe.to_string_lossy().into_owned();
        let entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=OrangSign\n\
             GenericName=Code Signing Tool\n\
             Comment=Sign files with the OrangSign certificate\n\
             Exec={} %F\n\
             Icon=orangsign\n\
             Terminal=false\n\
             Categories=Development;Utility;Security;\n\
             StartupWMClass={APP_ID}\n",
            desktop_quote(&exe_str)
        );
        let _ = write_file(&user_desktop, &entry, 0o644);
    }
}

fn strip_pem_headers(raw: &str) -> String {
    let mut pem = String::new();
    let mut in_cert = false;
    for line in raw.lines() {
        if line.starts_with("-----BEGIN CERTIFICATE") {
            in_cert = true;
        }
        if in_cert {
            pem.push_str(line);
            pem.push('\n');
        }
        if line.starts_with("-----END CERTIFICATE") {
            break;
        }
    }
    pem
}

fn extract_pfx(pfx: &Path, dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    let pass = format!("env:{PASS_ENV}");

    let try_extract = |legacy: bool| -> Result<()> {
        let mut cmd = openssl();
        cmd.arg("pkcs12")
            .arg("-in")
            .arg(pfx)
            .args(["-passin", &pass, "-nokeys", "-clcerts", "-out"])
            .arg(&cert);
        if legacy {cmd.arg("-legacy");}
        run(&mut cmd)?;

        let mut cmd = openssl();
        cmd.arg("pkcs12")
            .arg("-in")
            .arg(pfx)
            .args(["-passin", &pass, "-nocerts", "-noenc", "-out"])
            .arg(&key);
        if legacy {cmd.arg("-legacy");}
        run(&mut cmd)?;
        Ok(())
    };

    if let Err(err) = try_extract(false) {
        try_extract(true)
            .map_err(|_| err)
            .context("cannot read the certificate (wrong password or corrupt cert.pfx?)")?;
    }

    Ok((cert, key))
}

pub fn cert_pem(pfx: &Path) -> Result<String> {
    let tmp = TempDir::new()?;
    let (cert, _) = extract_pfx(pfx, &tmp.0)?;
    let raw = fs::read_to_string(cert)?;
    Ok(strip_pem_headers(&raw))
}

pub fn cert_info(pfx: &Path) -> Result<String> {
    let tmp = TempDir::new()?;
    let (cert, _) = extract_pfx(pfx, &tmp.0)?;
    run(openssl()
        .args(["x509","-noout","-subject","-enddate","-fingerprint","-sha256","-in",])
        .arg(&cert))
}

pub fn create_certificate(log: &mut Log) -> Result<()> {
    let path = cert_path();
    let tmp = TempDir::new()?;
    let key = tmp.0.join("key.pem");
    let cert = tmp.0.join("cert.pem");

    log.push("Creating new certificate...".into());

    run(openssl()
        .args(["req", "-x509", "-newkey", "rsa:2048", "-sha256", "-days", "1825", "-noenc",])
        .arg("-keyout")
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .args([
            "-subj",
            CERT_SUBJECT,
            "-addext",
            "basicConstraints=critical,CA:TRUE,pathlen:0",
            "-addext",
            "keyUsage=critical,digitalSignature,keyCertSign",
            "-addext",
            "extendedKeyUsage=codeSigning",
        ]))?;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }

    log.push(format!("Exporting certificate to {}", path.display()));
    let tmp_pfx = path.with_extension("pfx.new");

    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp_pfx)?;

    run(openssl()
        .args(["pkcs12", "-export", "-name", "OrangSign"])
        .arg("-inkey")
        .arg(&key)
        .arg("-in")
        .arg(&cert)
        .args(["-passout", &format!("env:{PASS_ENV}"), "-out"])
        .arg(&tmp_pfx))?;

    fs::rename(&tmp_pfx, &path)?;

    if !path.is_file() {
        bail!("Failed to create certificate file");
    }
    if let Ok(info) = cert_info(&path) {
        for line in info.lines() {
            log.push(line.to_string());
        }
    }
    log.push("Certificate created successfully!".into());
    Ok(())
}

fn is_authenticode_target(file: &Path) -> bool {
    const EXTS: &[&str] = &["exe", "dll", "sys", "ocx", "scr", "efi", "cpl", "drv", "msi", "msp", "cab", "cat", "appx", "msix", "ps1", "psm1", "psd1", "ps1xml", "js", "vbs", "wsf",];
    if let Some(ext) = file.extension().and_then(OsStr::to_str) {
        if EXTS.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
            return true;
        }
    }

    let mut magic = [0u8; 2];
    if let Ok(mut f) = fs::File::open(file) {
        if f.read_exact(&mut magic).is_ok() && &magic == b"MZ" {
            return true;
        }
    }
    false
}

fn sign_authenticode(
    osslsigncode: &Path,
    file: &Path,
    cert: &Path,
    key: &Path,
    log: &mut Log,
) -> Result<()> {
    let fname = file.file_name().unwrap_or_default().to_string_lossy();
    let out = file.with_file_name(format!(".{fname}.orangsign-tmp"));

    let run_sign = |timestamp: bool| -> Result<String> {
        let _ = fs::remove_file(&out);
        let mut cmd = Command::new(osslsigncode);
        cmd.arg("sign")
            .arg("-certs")
            .arg(cert)
            .arg("-key")
            .arg(key)
            .args(["-h", "sha256", "-n", "OrangSign"]);
        if timestamp {
            cmd.args(["-ts", TIMESTAMP_URL]);
        }
        cmd.arg("-in").arg(file).arg("-out").arg(&out);
        run(&mut cmd)
    };

    let res = match run_sign(true) {
        Ok(o) => Ok(o),
        Err(e) => {
            log.push(format!("{e:#}"));
            log.push("Retrying without timestamp server...".into());
            run_sign(false)
        }
    };

    let output = match res {
        Ok(o) => o,
        Err(e) => {
            let _ = fs::remove_file(&out);
            return Err(e);
        }
    };

    for line in output.lines() {
        if !line.trim().is_empty() {log.push(line.to_string());}
    }

    if let Ok(meta) = fs::metadata(file) {
        let _ = fs::set_permissions(&out, meta.permissions());
    }
    fs::rename(&out, file).with_context(|| format!("cannot replace {}", file.display()))?;
    log.push("Embedded Authenticode signature".into());
    Ok(())
}

fn sign_detached(file: &Path, cert: &Path, key: &Path, log: &mut Log) -> Result<PathBuf> {
    let mut sig_os = file.as_os_str().to_owned();
    sig_os.push(".p7s");
    let sig = PathBuf::from(sig_os);

    run(openssl()
        .args([
            "cms", "-sign", "-binary", "-md", "sha256", "-outform", "DER",
        ])
        .arg("-in")
        .arg(file)
        .arg("-signer")
        .arg(cert)
        .arg("-inkey")
        .arg(key)
        .arg("-out")
        .arg(&sig))?;

    log.push(format!("wrote detached signature {}", sig.display()));
    log.push(format!(
        "verify with openssl cms -verify -binary -inform DER -in \"{}\" -content \"{}\" -purpose any -out /dev/null",
        sig.display(),
        file.display()
    ));
    Ok(sig)
}

pub fn sign_file(target: &Path, log: &mut Log) -> Result<()> {
    let pfx = cert_path();
    if !target.exists() {bail!("Path not found: {}", target.display());
    }
    if !pfx.is_file() {bail!("Certificate not found. Please create certificate first.");}

    let tmp = TempDir::new()?;
    let (cert, key) = extract_pfx(&pfx, &tmp.0)?;
    let osslsigncode = which("osslsigncode");

    if target.is_dir() {
        let mut files = Vec::new();
        collect_signable_files(target, &mut files)?;
        if files.is_empty() {bail!("No signable files found in folder: {}", target.display());}

        log.push(format!(
            "Signing {} file(s) in {}...",
            files.len(),
            target.display()
        ));
        for file in &files {sign_single_file(file, &cert, &key, osslsigncode.as_deref(), log)?;}
        log.push(format!("All {} file(s) signed successfully!", files.len()));
        Ok(())
    } else if target.is_file() {
        sign_single_file(target, &cert, &key, osslsigncode.as_deref(), log)?;
        log.push("File signed successfully!".into());
        Ok(())
    } else {bail!("Unsupported path: {}", target.display());}
}

fn collect_signable_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| format!("cannot read directory {}", dir.display()))?
    {
        entries.push(entry?);
    }
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {continue;}
        let path = entry.path();
        let ft = entry
            .file_type()
            .with_context(|| format!("cannot inspect {}", path.display()))?;

        if ft.is_dir() {
            collect_signable_files(&path, out)?;
        } else if ft.is_file() {
            let is_sig = path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|ext| ext.eq_ignore_ascii_case("p7s"));
            if !is_sig {
                out.push(path);
            }
        }
    }
    Ok(())
}

fn sign_single_file(
    file: &Path,
    cert: &Path,
    key: &Path,
    osslsigncode: Option<&Path>,
    log: &mut Log,
) -> Result<()> {
    log.push(format!("Signing {}...", file.display()));
    if is_authenticode_target(file) {
        if let Some(tool) = osslsigncode {
            log.push(format!("Using {}", tool.display()));
            return sign_authenticode(tool, file, cert, key, log);
        }
        log.push(
            "Windows binary detected but osslsigncode is not installed \
             (sudo pacman -S osslsigncode) - creating a detached signature instead."
                .into(),
        );
    }
    sign_detached(file, cert, key, log)?;
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemState {
    pub installed: bool,
    pub context_menu: bool,
    pub trusted: Option<bool>,
    pub cert_info: Option<String>,
    pub has_osslsigncode: bool,
}

pub fn system_state() -> SystemState {
    let pfx = cert_path();
    let has_pfx = pfx.is_file();
    let pem = if has_pfx { cert_pem(&pfx).ok() } else { None };
    let trusted = pem.as_ref().map(|pem| {
        TRUST_STORES.iter().any(|(dir, _, _)| {
            if let Ok(t) = fs::read_to_string(Path::new(dir).join(TRUST_FILE)) {
                strip_pem_headers(&t).trim() == pem.trim()
            } else {
                false
            }
        })
    });

    SystemState {
        installed: is_installed(),
        context_menu: context_menu_files().iter().any(|(p, _)| p.exists()),
        trusted,
        cert_info: if has_pfx { cert_info(&pfx).ok() } else { None },
        has_osslsigncode: which("osslsigncode").is_some(),
    }
}

fn desktop_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn shell_quote(s: &str) -> String {format!("'{}'", s.replace('\'', r"'\''"))}

fn context_menu_files() -> Vec<(PathBuf, String)> {
    let exe = installed_exe().to_string_lossy().into_owned();
    let data = xdg_data_home();
    vec![
        (
            data.join("kio/servicemenus/orangsign.desktop"),
            format!(
                "[Desktop Entry]\n\
                 Type=Service\n\
                 MimeType=all/allfiles;inode/directory;\n\
                 Actions=orangsign-sign;\n\
                 X-KDE-ServiceTypes=KonqPopupMenu/Plugin\n\
                 X-KDE-Priority=TopLevel\n\
                 \n\
                 [Desktop Action orangsign-sign]\n\
                 Name={CONTEXT_MENU_TEXT}\n\
                 Icon=orangsign\n\
                 Exec={} --sign %F\n",
                desktop_quote(&exe)
            ),
        ),
        (
            data.join("nemo/actions/orangsign.nemo_action"),
            format!(
                "[Nemo Action]\n\
                 Name={CONTEXT_MENU_TEXT}\n\
                 Comment=Sign the selected files or folders with the OrangSign certificate\n\
                 Exec={} --sign %F\n\
                 Icon-Name=orangsign\n\
                 Selection=notnone\n\
                 Extensions=any;\n",
                desktop_quote(&exe)
            ),
        ),
        (
            data.join("nautilus/scripts").join(CONTEXT_MENU_TEXT),
            format!("#!/bin/sh\nexec {} --sign \"$@\"\n", shell_quote(&exe)),
        ),
    ]
}

fn file_manager_for(path: &Path) -> &'static str {
    let s = path.to_string_lossy();
    if s.contains("/kio/") {
        "dolphin"
    } else if s.contains("/nemo/") {
        "nemo"
    } else {
        "nautilus"
    }
}

pub fn add_context_menu(log: &mut Log) -> Result<()> {
    if !is_installed() {bail!("OrangSign must be installed into the system before adding it to context menu.");}

    let files = context_menu_files();
    let mut targets: Vec<_> = files
        .iter()
        .filter(|(p, _)| which(file_manager_for(p)).is_some())
        .collect();

    if targets.is_empty() {targets.push(&files[0]);}

    for (path, contents) in targets {
        write_file(path, contents, 0o755)?;
        log.push(format!("[{}] {}", file_manager_for(path), path.display()));
    }

    log.push(format!(
        "added \"{CONTEXT_MENU_TEXT}\" to the right-click menu (points at {}).",
        installed_exe().display()
    ));
    log.push("Restart file manager if entry doesn't show up right away.".into());
    Ok(())
}

pub fn remove_context_menu(log: &mut Log) -> Result<()> {
    for (path, _) in context_menu_files() {
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
            log.push(format!("Removed {}", path.display()));
        }
    }
    log.push("context menu entry removed.".into());
    Ok(())
}

fn refresh_context_menu(log: &mut Log) -> Result<()> {
    for (path, contents) in context_menu_files() {
        if path.exists() {
            write_file(&path, &contents, 0o755)?;
        }
    }
    log.push(format!(
        "context menu now points at {}",
        installed_exe().display()
    ));
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Install,
    Uninstall,
    Trust,
    Untrust,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Uninstall => "uninstall",
            Action::Trust => "trust",
            Action::Untrust => "untrust",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "install" => Some(Action::Install),
            "uninstall" => Some(Action::Uninstall),
            "trust" => Some(Action::Trust),
            "untrust" => Some(Action::Untrust),
            _ => None,
        }
    }
}

pub fn run_elevated(action: Action, log: &mut Log) -> Result<()> {
    let pkexec = which("pkexec").ok_or_else(|| anyhow!("pkexec (polkit) is not installed"))?;
    let exe = current_exe();
    if !exe.is_absolute() {bail!("cannot determine the path of the running executable");}

    log.push("Requesting administrator rights (polkit)...".into());
    let out = Command::new(pkexec)
        .arg(&exe)
        .args(["--elevated", action.name(), "--cert"])
        .arg(cert_path())
        .output()
        .context("failed to start pkexec")?;

    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if !line.is_empty() {
            log.push(line.to_string());
        }
    }

    match out.status.code() {
        Some(0) => Ok(()),
        Some(126) => bail!("Authentication was cancelled."),
        Some(127) => bail!("Not authorized (authentication failed)."),
        _ => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if stderr.is_empty() {
                bail!("Elevated operation failed");
            } else {
                bail!("Elevated operation failed: {stderr}");
            }
        }
    }
}

pub fn install(log: &mut Log) -> Result<()> {
    let dir = data_dir();
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;

    let portable = exe_dir().join(CERT_FILE);
    let user_cert = dir.join(CERT_FILE);
    let legacy_cert = xdg_data_home().join(APP_ID).join(CERT_FILE);
    if portable.is_file() && !user_cert.exists() {
        fs::copy(&portable, &user_cert)?;
        fs::set_permissions(&user_cert, fs::Permissions::from_mode(0o600))?;
        log.push(format!("Copied certificate to {}", user_cert.display()));
    } else if legacy_cert.is_file() && !user_cert.exists() {
        fs::copy(&legacy_cert, &user_cert)?;
        fs::set_permissions(&user_cert, fs::Permissions::from_mode(0o600))?;
        log.push(format!("Migrated certificate to {}", user_cert.display()));
    }

    let me = current_exe();
    let target = installed_exe();
    if fs::canonicalize(&me).ok() == fs::canonicalize(&target).ok() {
        log.push("already running from install location.".into());
    } else {
        let tmp = target.with_extension("new");
        fs::copy(&me, &tmp).with_context(|| format!("cannot copy to {}", tmp.display()))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
        fs::rename(&tmp, &target)?;
        log.push(format!(
            "Installed {} -> {}",
            me.display(),
            target.display()
        ));
    }

    let data = xdg_data_home();
    let icon_path = dir.join("orangsign.png");
    write_bytes(&icon_path, LOGO_PNG, 0o644)?;
    write_bytes(
        &data.join("icons/hicolor/256x256/apps/orangsign.png"),
        LOGO_PNG,
        0o644,
    )?;

    let target_str = target.to_string_lossy().into_owned();
    let desktop_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=OrangSign\n\
         GenericName=Code Signing Tool\n\
         Comment=Sign files with the OrangSign certificate\n\
         Exec={} %F\n\
         Icon=orangsign\n\
         Terminal=false\n\
         Categories=Development;Utility;Security;\n\
         StartupWMClass={APP_ID}\n",
        desktop_quote(&target_str)
    );
    let user_desktop = data.join("applications/orangsign.desktop");
    write_file(&user_desktop, &desktop_entry, 0o644)?;
    log.push(format!(
        "Added application menu entry {}",
        user_desktop.display()
    ));

    let _ = Command::new("update-desktop-database")
        .arg(data.join("applications"))
        .output();
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-q", "-t", "-f"])
        .arg(data.join("icons/hicolor"))
        .output();

    if context_menu_files().iter().any(|(p, _)| p.exists()) {
        refresh_context_menu(log)?;
    }

    log.push("OrangSign installed.".into());
    Ok(())
}

pub fn uninstall(log: &mut Log) -> Result<()> {
    let data = xdg_data_home();
    let dir = data_dir();
    for path in [
        installed_exe(),
        dir.join("orangsign.png"),
        data.join("applications/orangsign.desktop"),
        data.join("icons/hicolor/256x256/apps/orangsign.png"),
    ] {
        match fs::remove_file(&path) {
            Ok(()) => log.push(format!("Removed {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("cannot remove {}", path.display())),
        }
    }

    let _ = Command::new("update-desktop-database")
        .arg(data.join("applications"))
        .output();

    if context_menu_files().iter().any(|(p, _)| p.exists()) {
        remove_context_menu(log)?;
    }
    log.push(format!("Your certificate in {} was kept.", dir.display()));
    Ok(())
}

pub fn elevated_main(action: Action, cert: Option<PathBuf>) -> i32 {
    let mut log = Log::new();
    let result = match action {
        Action::Install => install(&mut log),
        Action::Uninstall => {
            let _ = do_legacy_uninstall(&mut log);
            uninstall(&mut log)
        }
        Action::Trust => match cert {
            Some(c) => do_trust(&c, &mut log),
            None => Err(anyhow!("no certificate given")),
        },
        Action::Untrust => do_untrust(&mut log),
    };

    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            log.push(format!("Error: {e:#}"));
            1
        }
    };
    for line in log {
        println!("{line}");
    }
    code
}

fn do_legacy_uninstall(log: &mut Log) -> Result<()> {
    for path in [
        LEGACY_INSTALL_BIN,
        LEGACY_INSTALL_DESKTOP,
        LEGACY_INSTALL_ICON,
        LEGACY_INSTALL_PIXMAP,
        LEGACY_INSTALL_SVG,
    ] {
        match fs::remove_file(path) {
            Ok(()) => log.push(format!("Removed {path}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("cannot remove {path}")),
        }
    }
    let _ = Command::new("update-desktop-database")
        .arg("/usr/local/share/applications")
        .output();
    Ok(())
}

fn trust_store() -> Result<(&'static str, &'static str, &'static [&'static str])> {
    for &(dir, tool, args) in TRUST_STORES {
        if Path::new(dir).parent().is_some_and(Path::is_dir) && which(tool).is_some() {return Ok((dir, tool, args));}
    }
    bail!("no supported system trust store found (need update-ca-trust or update-ca-certificates)")
}

fn rebuild_trust(tool: &str, args: &[&str], log: &mut Log) -> Result<()> {
    log.push(format!("Running {tool} {}", args.join(" ")));
    run(Command::new(tool).args(args))?;
    Ok(())
}

fn do_trust(pfx: &Path, log: &mut Log) -> Result<()> {
    if !pfx.is_file() {bail!("Certificate not found. Please create a certificate first.");}
    let pem = cert_pem(pfx)?;
    if pem.is_empty() {bail!("no certificate found in {}", pfx.display());}
    let (dir, tool, args) = trust_store()?;
    let anchor = Path::new(dir).join(TRUST_FILE);
    write_file(&anchor, &pem, 0o644)?;
    log.push(format!("Added trust anchor {}", anchor.display()));

    rebuild_trust(tool, args, log)?;
    if let Ok(info) = cert_info(pfx) {
        for line in info.lines() {log.push(line.to_string());}
    }
    log.push("Certificate is now trusted system-wide.".into());
    Ok(())
}

fn do_untrust(log: &mut Log) -> Result<()> {
    let mut removed = false;
    for &(dir, _, _) in TRUST_STORES {
        let anchor = Path::new(dir).join(TRUST_FILE);
        if anchor.exists() {
            fs::remove_file(&anchor)?;
            log.push(format!("Removed {}", anchor.display()));
            removed = true;
        }
    }
    if removed {
        let (_, tool, args) = trust_store()?;
        if tool == "update-ca-certificates" {
            rebuild_trust(tool, &["--fresh"], log)?;
        } else {
            rebuild_trust(tool, args, log)?;
        }
    }
    log.push("Certificate is no longer trusted system-wide.".into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(desktop_quote("/a b/$x"), r#""/a b/\$x""#);
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn authenticode_detection() {
        assert!(is_authenticode_target(Path::new("/nonexistent/setup.EXE")));
        assert!(!is_authenticode_target(Path::new(
            "/nonexistent/readme.txt"
        )));
    }

    #[test]
    fn cert_and_signing_end_to_end() {
        let tmp = TempDir::new().unwrap();
        unsafe {std::env::set_var("XDG_DATA_HOME", &tmp.0);}
        let mut log = Log::new();
        create_certificate(&mut log).expect("create_certificate failed");
        assert!(cert_path().is_file(), "cert.pfx should exist");
        let sample = tmp.0.join("sample.bin");
        fs::write(&sample, b"hello orangsign").unwrap();
        let mut sign_log = Log::new();
        sign_file(&sample, &mut sign_log).expect("sign_file failed");
        assert!(
            tmp.0.join("sample.bin.p7s").is_file(),
            "detached signature should exist"
        );
        let folder = tmp.0.join("bundle");
        let nested = folder.join("sub");
        fs::create_dir_all(&nested).unwrap();
        fs::write(folder.join("a.bin"), b"file a").unwrap();
        fs::write(nested.join("b.bin"), b"file b").unwrap();
        fs::write(folder.join(".hidden"), b"skip me").unwrap();
        let mut dir_log = Log::new();
        sign_file(&folder, &mut dir_log).expect("folder sign_file failed");
        assert!(folder.join("a.bin.p7s").is_file(), "a.bin.p7s should exist");
        assert!(nested.join("b.bin.p7s").is_file(), "b.bin.p7s should exist");
        assert!(
            !folder.join(".hidden.p7s").exists(),
            "hidden file should not be signed"
        );
        assert!(
            !folder.join("a.bin.p7s.p7s").exists(),
            ".p7s file should not be signed"
        );
    }
}