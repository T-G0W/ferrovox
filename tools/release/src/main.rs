use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: String,
    setup: String,
    sha256: String,
    size: u64,
    notes_de: String,
    notes_en: String,
}

type Res<T> = Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("keygen") => keygen(&args[1..]),
        Some("sign") => sign(&args[1..]),
        Some("verify") => verify(&args[1..]),
        Some("licenses") => licenses(&args[1..]),
        _ => Err("Befehle: keygen | sign | verify | licenses".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn req(args: &[String], name: &str) -> Res<String> {
    opt(args, name).ok_or_else(|| format!("{name} fehlt"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Res<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err("ungültiges Hex".into());
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string())).collect()
}

fn keypair(seed_hex: &str) -> Res<ed25519_compact::KeyPair> {
    let seed = ed25519_compact::Seed::from_slice(&unhex(seed_hex)?).map_err(|e| e.to_string())?;
    Ok(ed25519_compact::KeyPair::from_seed(seed))
}

fn keygen(args: &[String]) -> Res<()> {
    let path = PathBuf::from(args.first().ok_or("Zieldatei fehlt")?);
    if path.exists() {
        return Err(format!("{} existiert bereits – wird nicht überschrieben", path.display()));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| e.to_string())?;
    let kp = keypair(&hex(&seed))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, hex(&seed)).map_err(|e| e.to_string())?;
    println!("{}", hex(kp.pk.as_ref()));
    Ok(())
}

fn sha256_file(path: &Path) -> Res<(String, u64)> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((hex(&Sha256::digest(&bytes)), bytes.len() as u64))
}

fn sign(args: &[String]) -> Res<()> {
    let version = req(args, "--version")?;
    let setup = PathBuf::from(req(args, "--setup")?);
    let out = PathBuf::from(req(args, "--out")?);
    let seed = match opt(args, "--key") {
        Some(p) => std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?,
        None => std::env::var("FERROVOX_SIGNING_KEY").map_err(|_| "kein Schlüssel (--key oder FERROVOX_SIGNING_KEY)")?,
    };
    let kp = keypair(&seed)?;
    let (sha256, size) = sha256_file(&setup)?;
    let manifest = Manifest {
        version,
        setup: setup.file_name().and_then(|n| n.to_str()).ok_or("Setup-Name")?.to_string(),
        sha256,
        size,
        notes_de: opt(args, "--notes-de").unwrap_or_default(),
        notes_en: opt(args, "--notes-en").unwrap_or_default(),
    };
    let body = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let sig = kp.sk.sign(&body, None);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::write(out.join("latest.json"), &body).map_err(|e| e.to_string())?;
    std::fs::write(out.join("latest.json.sig"), hex(sig.as_ref())).map_err(|e| e.to_string())?;
    println!("latest.json signiert ({}, {} Bytes, Schlüssel {})", manifest.setup, size, &hex(kp.pk.as_ref())[..16]);
    Ok(())
}

fn verify(args: &[String]) -> Res<()> {
    let dir = PathBuf::from(req(args, "--dir")?);
    let pk = ed25519_compact::PublicKey::from_slice(&unhex(&req(args, "--public")?)?).map_err(|e| e.to_string())?;
    let body = std::fs::read(dir.join("latest.json")).map_err(|e| e.to_string())?;
    let sig_hex = std::fs::read_to_string(dir.join("latest.json.sig")).map_err(|e| e.to_string())?;
    let sig = ed25519_compact::Signature::from_slice(&unhex(&sig_hex)?).map_err(|e| e.to_string())?;
    pk.verify(&body, &sig).map_err(|_| "Signatur ungültig".to_string())?;
    let m: Manifest = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let (sha, size) = sha256_file(&dir.join(&m.setup))?;
    if sha != m.sha256 || size != m.size {
        return Err("Setup passt nicht zur Beschreibung".into());
    }
    println!("OK: {} {} ({} Bytes)", m.setup, m.version, m.size);
    Ok(())
}

fn licenses(args: &[String]) -> Res<()> {
    let manifest = req(args, "--manifest")?;
    let target = req(args, "--target")?;
    let out_path = PathBuf::from(req(args, "--out")?);
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked", "--filter-platform", &target, "--manifest-path", &manifest])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let meta: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let root = meta["resolve"]["root"].as_str().ok_or("kein Root-Paket")?.to_string();
    let nodes: HashMap<&str, &Value> =
        meta["resolve"]["nodes"].as_array().ok_or("keine Nodes")?.iter().filter_map(|n| Some((n["id"].as_str()?, n))).collect();

    let mut used = BTreeSet::new();
    let mut queue = VecDeque::from([root.as_str()]);
    while let Some(id) = queue.pop_front() {
        let Some(node) = nodes.get(id) else { continue };
        for dep in node["deps"].as_array().into_iter().flatten() {
            let normal = dep["dep_kinds"].as_array().into_iter().flatten().any(|k| k["kind"].is_null());
            if let (true, Some(pkg)) = (normal, dep["pkg"].as_str()) {
                if used.insert(pkg.to_string()) {
                    queue.push_back(pkg);
                }
            }
        }
    }

    struct Entry {
        line: String,
        texts: Vec<String>,
    }
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    for pkg in meta["packages"].as_array().ok_or("keine Pakete")? {
        let id = pkg["id"].as_str().unwrap_or_default();
        if !used.contains(id) {
            continue;
        }
        let name = pkg["name"].as_str().unwrap_or("?");
        let version = pkg["version"].as_str().unwrap_or("?");
        let license = pkg["license"].as_str().unwrap_or("siehe Lizenztext / see license text");
        let repo = pkg["repository"].as_str().unwrap_or("");
        let dir = Path::new(pkg["manifest_path"].as_str().unwrap_or_default()).parent().map(Path::to_path_buf).unwrap_or_default();
        let mut texts = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .filter(|p| {
                    let n = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_ascii_uppercase();
                    ["LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE"].iter().any(|k| n.starts_with(k))
                })
                .collect();
            files.sort();
            for f in files {
                if let Ok(t) = std::fs::read_to_string(&f) {
                    texts.push(t.replace("\r\n", "\n").trim().to_string());
                }
            }
        }
        let line = format!("{name} {version} — {license}{}", if repo.is_empty() { String::new() } else { format!(" — {repo}") });
        entries.insert(format!("{name}@{version}"), Entry { line, texts });
    }

    let mut by_text: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut without = Vec::new();
    for (key, e) in &entries {
        if e.texts.is_empty() {
            without.push(key.clone());
        }
        for t in &e.texts {
            by_text.entry(t.clone()).or_default().push(key.clone());
        }
    }

    let mut out = String::new();
    out.push_str("Ferrovox – Lizenzen der verwendeten Open-Source-Bibliotheken\n");
    out.push_str("Ferrovox – licenses of the open-source libraries it uses\n");
    out.push_str("============================================================\n\n");
    out.push_str("Ferrovox © 2026 T-GOW. Die folgenden Bibliotheken stehen unter ihren eigenen Lizenzen.\n");
    out.push_str("The following libraries are distributed under their own licenses.\n\n");
    for e in entries.values() {
        out.push_str(&e.line);
        out.push('\n');
    }
    if !without.is_empty() {
        out.push_str("\nOhne eigene Lizenzdatei im Paket (Lizenz laut Paketangabe oben) / no license file shipped in the package:\n");
        out.push_str(&without.join(", "));
        out.push('\n');
    }
    for (text, users) in &by_text {
        out.push_str("\n\n------------------------------------------------------------\n");
        out.push_str(&format!("{}\n", users.join(", ")));
        out.push_str("------------------------------------------------------------\n\n");
        out.push_str(text);
        out.push('\n');
    }
    if let Some(dir) = out_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out_path, out).map_err(|e| e.to_string())?;
    println!("{} Bibliotheken, {} verschiedene Lizenztexte → {}", entries.len(), by_text.len(), out_path.display());
    Ok(())
}
