//! Prompt templates and persona management
//!
//! Supports loading AI personas/system prompts from TOML files.
//!
//! # Example Prompt File
//!
//! ```toml
//! [persona]
//! name = "Business Analyst"
//! description = "AI assistant for analyzing business data"
//!
//! [system_prompt]
//! content = """
//! You are a skilled business analyst...
//! """
//!
//! [examples]
//! questions = ["What were our sales today?", "Show inventory status"]
//!
//! [tools]
//! primary = ["search_orders", "get_inventory"]
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// A persona/prompt template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Persona metadata
    pub persona: PersonaInfo,

    /// The system prompt
    pub system_prompt: SystemPrompt,

    /// Example questions this persona handles well
    #[serde(default)]
    pub examples: PromptExamples,

    /// Tools commonly used by this persona
    #[serde(default)]
    pub tools: PromptTools,
}

/// Persona metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaInfo {
    /// Display name of the persona
    pub name: String,

    /// Brief description
    #[serde(default)]
    pub description: String,
}

/// System prompt content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPrompt {
    /// The full system prompt content
    pub content: String,
}

/// Example questions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptExamples {
    /// Questions this persona handles well
    #[serde(default)]
    pub questions: Vec<String>,
}

/// Tools commonly used
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptTools {
    /// Primary tools for this persona
    #[serde(default)]
    pub primary: Vec<String>,

    /// Secondary/supporting tools
    #[serde(default)]
    pub secondary: Vec<String>,
}

/// Manager for loading and caching prompt templates
#[derive(Debug)]
pub struct PromptManager {
    /// Directory containing prompt templates
    prompts_dir: PathBuf,

    /// Cached templates by name
    cache: HashMap<String, PromptTemplate>,
}

impl PromptManager {
    /// Create a new prompt manager
    pub fn new(prompts_dir: impl Into<PathBuf>) -> Self {
        Self {
            prompts_dir: prompts_dir.into(),
            cache: HashMap::new(),
        }
    }

    /// Load a prompt template by name (file name without extension)
    pub async fn load(&mut self, name: &str) -> Result<&PromptTemplate, PromptError> {
        // Return cached if available
        if self.cache.contains_key(name) {
            return Ok(self.cache.get(name).unwrap());
        }

        // Try to load from file
        let path = self.prompts_dir.join(format!("{}.toml", name));
        let template = Self::load_from_file(&path).await?;

        self.cache.insert(name.to_string(), template);
        Ok(self.cache.get(name).unwrap())
    }

    /// Load a template directly from a file path
    pub async fn load_from_file(path: &Path) -> Result<PromptTemplate, PromptError> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| PromptError::IoError(e.to_string()))?;

        toml::from_str(&content).map_err(|e| PromptError::ParseError(e.to_string()))
    }

    /// List available prompts in the directory
    pub async fn list_available(&self) -> Result<Vec<String>, PromptError> {
        let mut prompts = Vec::new();

        let mut entries = fs::read_dir(&self.prompts_dir)
            .await
            .map_err(|e| PromptError::IoError(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| PromptError::IoError(e.to_string()))?
        {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "toml") {
                if let Some(stem) = path.file_stem() {
                    prompts.push(stem.to_string_lossy().to_string());
                }
            }
        }

        Ok(prompts)
    }

    /// Get prompt by name if cached
    pub fn get_cached(&self, name: &str) -> Option<&PromptTemplate> {
        self.cache.get(name)
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

/// Errors from prompt loading
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("IO error: {0}")]
    IoError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Prompt not found: {0}")]
    NotFound(String),
}

/// Built-in prompts that don't require files
pub mod builtin {
    /// Default general-purpose assistant prompt
    pub const DEFAULT: &str = "You are Moxie, a helpful AI assistant. You can use tools to help answer questions and complete tasks. Be concise and helpful in your responses.";

    /// Business analyst prompt (simplified version)
    pub const BUSINESS_ANALYST: &str = r#"You are a skilled business analyst assistant. Your role is to help business owners understand their data and make informed decisions.

When analyzing data:
1. Gather context first - use tools to get the actual data
2. Present data clearly with tables and key metrics
3. Provide insights, not just numbers - explain what it means
4. Suggest actionable next steps when appropriate

Format your responses as:
1. **Summary** - One sentence answer
2. **Key Metrics** - Important numbers
3. **Analysis** - What this means for the business
4. **Recommendations** - Suggested actions (if appropriate)

Always use actual data from tools - never make up numbers."#;

    /// Technical support prompt
    pub const TECH_SUPPORT: &str = r#"You are a technical support assistant. Help users troubleshoot issues with their systems.

When helping:
1. Ask clarifying questions to understand the problem
2. Use available tools to gather diagnostic information
3. Provide step-by-step solutions
4. Explain what caused the issue when possible

Be patient and clear in your explanations."#;

    /// Data entry assistant prompt
    pub const DATA_ENTRY: &str = r#"You are a data entry assistant. Help users input and manage data efficiently.

When handling data:
1. Confirm the data before making changes
2. Validate inputs against expected formats
3. Report any issues or anomalies
4. Summarize what was done after completion

Always ask for confirmation before writing or modifying data."#;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_template() {
        let toml_content = r#"
[persona]
name = "Test Persona"
description = "A test persona"

[system_prompt]
content = "You are a test assistant."

[examples]
questions = ["Hello?", "How are you?"]

[tools]
primary = ["tool_a", "tool_b"]
secondary = ["tool_c"]
"#;

        let template: PromptTemplate = toml::from_str(toml_content).unwrap();
        assert_eq!(template.persona.name, "Test Persona");
        assert_eq!(template.system_prompt.content, "You are a test assistant.");
        assert_eq!(template.examples.questions.len(), 2);
        assert_eq!(template.tools.primary.len(), 2);
    }

    #[test]
    fn test_minimal_template() {
        let toml_content = r#"
[persona]
name = "Minimal"

[system_prompt]
content = "Hello"
"#;

        let template: PromptTemplate = toml::from_str(toml_content).unwrap();
        assert_eq!(template.persona.name, "Minimal");
        assert!(template.examples.questions.is_empty());
        assert!(template.tools.primary.is_empty());
    }
}
