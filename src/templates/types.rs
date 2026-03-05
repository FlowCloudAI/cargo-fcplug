use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    #[serde(rename = "kind/llm")]
    LLM,
    #[serde(rename = "kind/image")]
    Image,
    #[serde(rename = "kind/tts")]
    TTS,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginInfo {
    pub id: String,
    pub version: String,
    pub author: String,
    pub abi_version: u32,
    pub name: String,
    pub description: String,
    pub kind: PluginKind,
}