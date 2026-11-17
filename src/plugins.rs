use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use boa_engine::{Context as JsContext, Source};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    pub manifest: Manifest,
    pub directory: PathBuf,
}

#[derive(Default)]
pub struct Manager {
    plugins: Vec<Plugin>,
    errors: Vec<String>,
}

impl Manager {
    pub fn load(root: &Path) -> Self {
        let directory = root.join("plugins");
        let mut manager = Self::default();
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return manager,
            Err(error) => {
                manager.errors.push(format!(
                    "{}: could not read plugin directory: {error}",
                    directory.display()
                ));
                return manager;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    manager
                        .errors
                        .push(format!("could not read plugin entry: {error}"));
                    continue;
                }
            };
            if !entry.path().is_dir() {
                continue;
            }
            if let Err(error) = manager.load_one(&entry.path()) {
                manager
                    .errors
                    .push(format!("{}: {error:#}", entry.path().display()));
            }
        }
        manager
    }

    fn load_one(&mut self, directory: &Path) -> Result<()> {
        let manifest_path = directory.join("plugin.json");
        let manifest: Manifest = serde_json::from_slice(
            &fs::read(&manifest_path).with_context(|| format!("could not read {}", manifest_path.display()))?,
        )
        .with_context(|| format!("invalid {}", manifest_path.display()))?;
        if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
            bail!("plugin name and version are required");
        }
        let entry = directory.join(&manifest.entry);
        let source = fs::read_to_string(&entry)
            .with_context(|| format!("could not read {}", entry.display()))?;
        let mut context = JsContext::default();
        context
            .eval(Source::from_bytes(source))
            .map_err(|error| anyhow::anyhow!("could not evaluate {}: {error}", entry.display()))?;
        self.plugins.push(Plugin {
            manifest,
            directory: directory.to_owned(),
        });
        Ok(())
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn loads_manifest_and_entry() {
        let root = tempdir().unwrap();
        let plugin = root.path().join("plugins").join("sample");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join("plugin.json"),
            r#"{"name":"Sample","version":"1.0.0","entry":"main.js"}"#,
        )
        .unwrap();
        fs::write(plugin.join("main.js"), "const value = 1 + 1;").unwrap();

        let manager = Manager::load(root.path());

        assert_eq!(manager.plugins()[0].manifest.name, "Sample");
        assert!(manager.errors().is_empty());
    }

    #[test]
    fn reports_invalid_javascript() {
        let root = tempdir().unwrap();
        let plugin = root.path().join("plugins").join("broken");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join("plugin.json"),
            r#"{"name":"Broken","version":"1.0.0","entry":"main.js"}"#,
        )
        .unwrap();
        fs::write(plugin.join("main.js"), "const = ;").unwrap();

        let manager = Manager::load(root.path());

        assert!(manager.plugins().is_empty());
        assert_eq!(manager.errors().len(), 1);
    }
}
