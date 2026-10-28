//! `cargo xtask sbom`: CycloneDX software bill of materials from `cargo metadata`.
//!
//! Writes JSON only (no secrets). Intended for release artifacts and local inventory.
//! Does not replace `cargo xtask licenses` (licence allow-list + `THIRD_PARTY_LICENSES.md`).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

struct Component {
    name: String,
    version: String,
    license: String,
    purl: String,
}

fn collect(meta: &Value) -> Vec<Component> {
    let members: Vec<&str> = meta["workspace_members"].as_array().into_iter().flatten().filter_map(|v| v.as_str()).collect();
    let mut out = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let id = p["id"].as_str().unwrap_or("");
        if members.contains(&id) || p["source"].is_null() {
            continue;
        }
        let name = p["name"].as_str().unwrap_or("").to_string();
        let version = p["version"].as_str().unwrap_or("").to_string();
        if name.is_empty() || version.is_empty() {
            continue;
        }
        let license = p["license"].as_str().unwrap_or("").trim().to_string();
        let purl = format!("pkg:cargo/{name}@{version}");
        out.push(Component { name, version, license, purl });
    }
    out.sort_by(|a, b| (a.name.to_lowercase(), &a.version).cmp(&(b.name.to_lowercase(), &b.version)));
    out
}

fn workspace_version(meta: &Value) -> String {
    let members: Vec<&str> = meta["workspace_members"].as_array().into_iter().flatten().filter_map(|v| v.as_str()).collect();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let id = p["id"].as_str().unwrap_or("");
        let name = p["name"].as_str().unwrap_or("");
        if members.contains(&id)
            && (name == "aad" || name == "xtask")
            && let Some(v) = p["version"].as_str()
        {
            return v.to_string();
        }
    }
    "0.0.0".into()
}

fn timestamp_rfc3339() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Second precision is enough for release artifacts; avoid pulling a time crate into xtask.
    format!("{secs}")
}

fn render(meta: &Value, components: &[Component]) -> Value {
    let version = workspace_version(meta);
    let comps: Vec<Value> = components
        .iter()
        .map(|c| {
            let mut component = json!({
                "type": "library",
                "name": c.name,
                "version": c.version,
                "purl": c.purl,
            });
            if !c.license.is_empty() {
                component["licenses"] = json!([{ "expression": c.license }]);
            }
            component
        })
        .collect();
    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "timestamp": timestamp_rfc3339(),
            "tools": [{
                "vendor": "O-Draft",
                "name": "cargo xtask sbom",
                "version": version,
            }],
            "component": {
                "type": "application",
                "name": "o-draft",
                "version": version,
                "purl": format!("pkg:generic/o-draft@{version}"),
            }
        },
        "components": comps,
    })
}

fn default_out(root: &Path, version: &str) -> PathBuf {
    root.join("dist").join("release").join(format!("o-draft-{version}.cdx.json"))
}

/// `cargo xtask sbom [--out PATH]`
pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let mut out_path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--out" => {
                let Some(p) = args.get(i + 1) else {
                    return Err("sbom: --out needs a path".into());
                };
                out_path = Some(PathBuf::from(p));
                i += 2;
            }
            "-h" | "--help" => {
                eprintln!("usage: cargo xtask sbom [--out PATH]");
                return Ok(());
            }
            other => return Err(format!("sbom: unknown argument `{other}`")),
        }
    }

    let out = crate::cargo().args(["metadata", "--format-version", "1", "--locked"]).output().map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: bad JSON: {e}"))?;
    let components = collect(&meta);
    let bom = render(&meta, &components);
    let version = workspace_version(&meta);
    let path = out_path.unwrap_or_else(|| default_out(root, &version));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&bom).map_err(|e| format!("sbom JSON: {e}"))?;
    std::fs::write(&path, format!("{text}\n")).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("sbom: {} components; wrote {}", components.len(), path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Component, collect, render};
    use serde_json::json;

    #[test]
    fn collect_skips_workspace_members() {
        let meta = json!({
            "workspace_members": ["path+file:///aad#aad@0.6.0"],
            "packages": [
                {
                    "id": "path+file:///aad#aad@0.6.0",
                    "name": "aad",
                    "version": "0.6.0",
                    "source": null,
                    "license": "MIT OR Apache-2.0"
                },
                {
                    "id": "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0",
                    "name": "serde",
                    "version": "1.0.0",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                    "license": "MIT OR Apache-2.0"
                }
            ]
        });
        let comps = collect(&meta);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].name, "serde");
        assert_eq!(comps[0].purl, "pkg:cargo/serde@1.0.0");
    }

    #[test]
    fn render_is_cyclonedx_without_secrets() {
        let meta = json!({
            "workspace_members": ["path+file:///aad#aad@0.6.0"],
            "packages": [{
                "id": "path+file:///aad#aad@0.6.0",
                "name": "aad",
                "version": "0.6.0",
                "source": null,
                "license": "MIT OR Apache-2.0"
            }]
        });
        let comps =
            [Component { name: "serde".into(), version: "1.0.0".into(), license: "MIT OR Apache-2.0".into(), purl: "pkg:cargo/serde@1.0.0".into() }];
        let bom = render(&meta, &comps);
        assert_eq!(bom["bomFormat"], "CycloneDX");
        assert_eq!(bom["specVersion"], "1.5");
        assert_eq!(bom["components"].as_array().map(|a| a.len()), Some(1));
        let text = bom.to_string();
        assert!(!text.contains("BEGIN PRIVATE KEY"));
        assert!(!text.contains("clientSecret"));
        assert!(!text.contains("WINDOWS_CERTIFICATE"));
    }
}
