use anyhow::Result;
use serde::{Serialize, Deserialize};
use crate::engine::{ModelCapability, EngineType};

/// A model entry in the catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub capability: ModelCapability,
    pub engine: EngineType,
    pub languages: Vec<String>,
    pub files: Vec<ModelFile>,
    #[serde(default)]
    pub preprocessing: Option<PreprocessingConfig>,
    /// If set, model is only available from this version onward
    #[serde(default)]
    pub available_from_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    pub filename: String,
    pub size_bytes: u64,
    /// Override HuggingFace repo (if different from model id)
    #[serde(default)]
    pub hf_repo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessingConfig {
    pub sample_rate: u32,
    pub feature_type: String,
    pub n_mels: Option<u32>,
}

/// An installed model on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModel {
    pub id: String,
    pub name: String,
    pub capability: ModelCapability,
    pub engine: EngineType,
    pub path: String,
    pub installed_at: String,
    pub size_bytes: u64,
}

/// Load the built-in model catalog
pub fn load_catalog() -> Result<Vec<CatalogModel>> {
    let catalog_json = include_str!("../../resources/registry.json");
    let catalog: CatalogContainer = serde_json::from_str(catalog_json)?;
    Ok(catalog.models)
}

#[derive(Deserialize)]
struct CatalogContainer {
    models: Vec<CatalogModel>,
}

/// Get the models directory path
pub fn models_dir() -> Result<std::path::PathBuf> {
    let app_support = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find Application Support directory"))?;
    Ok(app_support.join("TalkToMe").join("models"))
}

fn manifest_path() -> Result<std::path::PathBuf> {
    let path = models_dir()?.join("installed.json");
    Ok(path)
}

fn read_manifest() -> Result<Vec<InstalledModel>> {
    let path = manifest_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path)?;
    let models: Vec<InstalledModel> = serde_json::from_str(&data)?;
    Ok(models)
}

fn write_manifest(models: &[InstalledModel]) -> Result<()> {
    let path = manifest_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(models)?;
    std::fs::write(&path, data)?;
    Ok(())
}

/// List installed models from disk
pub fn list_installed_models(capability_filter: Option<&ModelCapability>) -> Result<Vec<InstalledModel>> {
    let models = read_manifest()?;

    Ok(match capability_filter {
        Some(cap) => models.into_iter().filter(|m| &m.capability == cap).collect(),
        None => models,
    })
}

/// Add a model to the installed manifest
pub fn add_installed_model(model: &InstalledModel) -> Result<()> {
    let mut models = read_manifest()?;
    // Remove existing entry with same id (re-download)
    models.retain(|m| m.id != model.id);
    models.push(model.clone());
    write_manifest(&models)?;
    Ok(())
}

/// Remove a model from the installed manifest
pub fn remove_installed_model(model_id: &str) -> Result<()> {
    let mut models = read_manifest()?;
    models.retain(|m| m.id != model_id);
    write_manifest(&models)?;
    Ok(())
}
