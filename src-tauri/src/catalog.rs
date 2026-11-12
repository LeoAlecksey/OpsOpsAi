//! The model catalog (catalog.json, sha256 pinned from Hugging Face metadata) and the verdict for
//! each model on this computer: recommended, possible (slow or tight on memory), or hidden.
//!
//! The verdict logic (`fit`) is adapted from OpsDeck `src-tauri/src/ai.rs` (same author).

use crate::hw::Hardware;
use serde::{Deserialize, Serialize};

const GIB: u64 = 1 << 30;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RemoteFile {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Model {
    pub id: String,
    pub title: String,
    pub family: String,
    pub params_b: f32,
    /// parameters working per token, billions: what decides the speed on a CPU (an MoE model is
    /// big on disk but quick, a dense one is as slow as it is big)
    pub active_b: f32,
    /// recommended memory, GB
    pub ram_gb: u32,
    pub license: String,
    pub description: String,
    pub vision: bool,
    pub file: RemoteFile,
    /// vision projector: lets the model see pictures
    pub mmproj: Option<RemoteFile>,
}

impl Model {
    /// Everything that is downloaded for this model.
    pub fn download_size(&self) -> u64 {
        self.file.size + self.mmproj.as_ref().map_or(0, |m| m.size)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EngineAsset {
    pub size: u64,
    pub sha256: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Engine {
    pub tag: String,
    pub assets: std::collections::BTreeMap<String, EngineAsset>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Catalog {
    pub schema: u32,
    pub engine: Engine,
    pub models: Vec<Model>,
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(include_str!("../catalog.json")).expect("catalog.json is valid"))
}

#[derive(Serialize, PartialEq, Debug, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// enough memory, normal speed
    Good,
    /// runs, but slowly on this computer
    Slow,
    /// less memory than recommended: runs, but squeezes everything else
    Tight,
    /// will not run or will not fit on the disk
    No,
}

#[derive(Serialize, PartialEq, Debug, Clone)]
pub struct Fit {
    pub level: Level,
    /// why, in words — empty when all is well
    pub why: String,
}

/// What awaits the model on this computer. Memory: the files fully loaded plus the context and the
/// system; disk: if it still has to be downloaded; speed: by the parameters active per token.
pub fn fit(m: &Model, hw: &Hardware, installed: bool) -> Fit {
    let gb = |b: u64| format!("{:.0}", (b as f64 / GIB as f64).max(1.0));
    let size = m.download_size();
    if !installed {
        if let Some(free) = hw.disk_free {
            let need = size + GIB;
            if free < need {
                return Fit { level: Level::No, why: format!("на диске свободно {} ГБ, а нужно около {} ГБ", gb(free), gb(need)) };
            }
        }
    }
    // the model files in memory + context and the system itself
    let min = size + 5 * GIB / 2;
    if hw.ram_total < min {
        return Fit { level: Level::No, why: format!("нужно не меньше {} ГБ памяти, а в компьютере {} ГБ", gb(min), gb(hw.ram_total)) };
    }
    // the OS reports a little less than is installed: a small margin
    if (hw.ram_total as f64) < m.ram_gb as f64 * GIB as f64 * 0.9 {
        return Fit {
            level: Level::Tight,
            why: format!("памяти впритык: {} ГБ при рекомендованных {} — другие программы станут медленнее", gb(hw.ram_total), m.ram_gb),
        };
    }
    if !hw.gpu && m.active_b >= 14.0 {
        return Fit { level: Level::Slow, why: "без видеокарты такая модель отвечает медленно — по несколько слов в секунду".into() };
    }
    Fit { level: Level::Good, why: String::new() }
}

#[cfg(test)]
pub(crate) fn hw_for_tests(ram_gb: u64, disk_gb: Option<u64>, gpu: bool) -> Hardware {
    Hardware { os: "linux", arch: "x86_64", ram_total: ram_gb * GIB, disk_free: disk_gb.map(|g| g * GIB), cpu_threads: 8, gpu, gpu_name: String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> &'static Model {
        catalog().models.iter().find(|m| m.id == id).unwrap()
    }

    #[test]
    fn the_catalog_is_complete_and_pinned() {
        let c = catalog();
        assert!(c.models.len() >= 5);
        for m in &c.models {
            for f in std::iter::once(&m.file).chain(m.mmproj.iter()) {
                assert_eq!(f.sha256.len(), 64, "{}: sha256", m.id);
                assert!(f.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}", m.id);
                assert!(f.url.starts_with("https://huggingface.co/"), "{}", m.id);
                assert!(f.size > 0, "{}", m.id);
            }
            assert!(!m.license.is_empty() && !m.description.is_empty(), "{}", m.id);
        }
        let ids: std::collections::HashSet<_> = c.models.iter().map(|m| &m.id).collect();
        assert_eq!(ids.len(), c.models.len(), "id моделей не повторяются");
        // every platform of the MVP has a pinned engine
        for a in ["bin-ubuntu-x64.tar.gz", "bin-ubuntu-arm64.tar.gz", "bin-macos-arm64.tar.gz", "bin-win-cpu-x64.zip"] {
            let name = format!("llama-{}-{a}", c.engine.tag);
            let e = c.engine.assets.get(&name).unwrap_or_else(|| panic!("нет {name}"));
            assert_eq!(e.sha256.len(), 64);
        }
    }

    #[test]
    fn an_8gb_laptop_gets_small_models_and_never_the_big_ones() {
        let hw = hw_for_tests(8, Some(200), false);
        assert_eq!(fit(model("qwen3.5-4b"), &hw, false).level, Level::Good);
        assert_eq!(fit(model("qwen3.5-9b"), &hw, false).level, Level::No, "6 ГБ файлов + контекст в 8 ГБ не влезут");
        assert_eq!(fit(model("qwen3.8-27b"), &hw, false).level, Level::No);
        assert_eq!(fit(model("qwen3.6-35b-a3b"), &hw, false).level, Level::No);
    }

    #[test]
    fn a_model_that_fits_below_the_recommended_memory_is_tight() {
        let hw = hw_for_tests(12, Some(200), false);
        let f = fit(model("qwen3.5-9b"), &hw, false);
        assert_eq!(f.level, Level::Tight);
        assert!(f.why.contains("впритык"), "{}", f.why);
    }

    #[test]
    fn a_big_dense_model_without_gpu_is_slow_but_moe_is_fine() {
        let hw = hw_for_tests(64, Some(500), false);
        assert_eq!(fit(model("qwen3.8-27b"), &hw, false).level, Level::Slow);
        assert_eq!(fit(model("qwen3.6-35b-a3b"), &hw, false).level, Level::Good, "MoE: активных параметров 3B");
        let gpu = hw_for_tests(64, Some(500), true);
        assert_eq!(fit(model("qwen3.8-27b"), &gpu, false).level, Level::Good);
    }

    #[test]
    fn no_disk_space_hides_a_model_that_is_not_installed_yet() {
        let hw = hw_for_tests(32, Some(3), false);
        let f = fit(model("qwen3.5-9b"), &hw, false);
        assert_eq!(f.level, Level::No);
        assert!(f.why.contains("на диске"), "{}", f.why);
        // already installed: the disk no longer matters
        assert_eq!(fit(model("qwen3.5-9b"), &hw, true).level, Level::Good);
    }
}
