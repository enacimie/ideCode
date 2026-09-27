use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::ClassModel;
use super::project::Project;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildResult {
    pub success: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    ToolMissing {
        tool: String,
        adapter: String,
        hint: String,
    },
    Io(String),
    Invalid(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::ToolMissing {
                tool,
                adapter,
                hint,
            } => write!(
                formatter,
                "No se encontró «{tool}», necesario para {adapter}. {hint}"
            ),
            AdapterError::Io(message) => write!(formatter, "{message}"),
            AdapterError::Invalid(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for AdapterError {}

impl From<std::io::Error> for AdapterError {
    fn from(error: std::io::Error) -> Self {
        AdapterError::Io(error.to_string())
    }
}

pub trait LanguageAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn compile_label(&self) -> &'static str {
        "Compilar"
    }
    fn entry_label(&self) -> &'static str {
        "Punto de entrada"
    }
    fn compile(&self, project: &Project) -> Result<BuildResult, AdapterError>;
    fn run_spec(
        &self,
        project: &Project,
        entry: Option<&str>,
        args: &[String],
    ) -> Result<RunSpec, AdapterError>;
    fn analyze(&self, project: &Project) -> Result<Vec<ClassModel>, AdapterError>;

    fn new_source(&self, name: &str) -> String {
        let _ = name;
        String::new()
    }

    fn validate_new_file(&self, name: &str) -> Result<(), String> {
        let _ = name;
        Ok(())
    }

    fn accepts(&self, name: &str) -> bool {
        let extension = Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        self.extensions()
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(extension))
    }
}
