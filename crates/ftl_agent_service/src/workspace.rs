use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ftl_agent_core::{ToolHost, required};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct WorkspaceHost {
    root: Option<PathBuf>,
    inner: Arc<dyn ToolHost>,
}
impl WorkspaceHost {
    pub fn new(root: Option<PathBuf>, inner: Arc<dyn ToolHost>) -> Result<Self> {
        let root = root.map(|p| p.canonicalize()).transpose()?;
        if root.as_ref().is_some_and(|p| !p.is_dir()) {
            bail!("workspace must be a directory")
        }
        Ok(Self { root, inner })
    }
    fn resolve(&self, path: &str) -> Result<PathBuf> {
        let root = self.root.as_ref().context("workspace access not granted")?;
        if Path::new(path).is_absolute() {
            bail!("relative workspace paths required")
        }
        let resolved = root.join(path).canonicalize()?;
        if !resolved.starts_with(root) {
            bail!("path leaves workspace")
        }
        Ok(resolved)
    }
}
#[async_trait]
impl ToolHost for WorkspaceHost {
    fn tools(&self) -> Vec<Value> {
        let mut tools = self.inner.tools();
        if self.root.is_some() {
            for (name, description) in [
                (
                    "workspace_list",
                    "List at most 1000 names under an approved workspace relative path.",
                ),
                (
                    "workspace_read",
                    "Read at most 65536 bytes from a UTF-8 file under the approved workspace.",
                ),
            ] {
                tools.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}));
            }
        }
        tools
    }
    fn cancel_run(&self, run_id: &str) {
        self.inner.cancel_run(run_id);
    }
    fn revoke(&self) {
        self.inner.revoke();
    }
    async fn call(&self, run_id: &str, name: &str, args: Value) -> Result<Value> {
        let result = match name {
            "workspace_read" => {
                let path = self.resolve(required(&args, "path")?)?;
                let file = std::fs::File::open(path)?;
                if !file.metadata()?.is_file() {
                    bail!("regular files only")
                }
                let mut bytes = Vec::new();
                file.take(65537).read_to_end(&mut bytes)?;
                if bytes.len() > 65536 {
                    bail!("file exceeds 65536 bytes")
                }
                String::from_utf8(bytes).context("UTF-8 text files only")?
            }
            "workspace_list" => {
                let path = self.resolve(required(&args, "path")?)?;
                let mut names = Vec::new();
                for entry in std::fs::read_dir(path)?.take(1001) {
                    names.push(entry?.file_name().to_string_lossy().into_owned());
                }
                if names.len() > 1000 {
                    bail!("directory exceeds 1000 entries")
                }
                names.sort();
                serde_json::to_string(&names)?
            }
            _ => return self.inner.call(run_id, name, args).await,
        };
        Ok(json!({"content":[{"type":"text","text":result}],"isError":false}))
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
