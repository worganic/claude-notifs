//! Configuration persistée dans %APPDATA%\claude-notifs\config.json.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PORT: u16 = 47790;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TypeConfig {
    pub key: String,
    pub label: String,
    pub enabled: bool,
    /// Durée d'affichage en secondes ; 0 = jusqu'au clic.
    pub duration: u32,
    pub color: String,
    /// Joue un son d'alerte à l'affichage.
    #[serde(default)]
    pub sound: bool,
    /// La notification clignote tant qu'elle est affichée.
    #[serde(default)]
    pub blink: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub position: Position,
    pub dnd: bool,
    pub max_visible: usize,
    pub types: Vec<TypeConfig>,
}

/// (clé, libellé, activé, durée, couleur, clignote) — l'ordre est celui affiché dans l'admin.
/// Le son est désactivé par défaut : des hooks sonores existent souvent déjà.
const DEFAULT_TYPES: &[(&str, &str, bool, u32, &str, bool)] = &[
    ("question", "Question posée", true, 0, "#e5a50a", true),
    ("plan", "Plan à valider", true, 0, "#9b59d0", true),
    ("permission", "Permission demandée", true, 0, "#e5533d", true),
    ("idle", "Claude attend ta réponse", true, 10, "#3b82f6", false),
    ("prompt_end", "Fin de prompt", true, 6, "#2ea043", false),
    ("prompt_start", "Début de prompt", true, 3, "#d97757", false),
    ("subagent_end", "Fin de sous-agent", false, 4, "#14b8a6", false),
    ("session_start", "Début de session", false, 4, "#64748b", false),
    ("session_end", "Fin de session", false, 4, "#64748b", false),
    ("notification", "Notification", true, 6, "#6366f1", false),
];

impl Default for Config {
    fn default() -> Self {
        Config {
            position: Position::default(),
            dnd: false,
            max_visible: 5,
            types: DEFAULT_TYPES
                .iter()
                .map(|&(key, label, enabled, duration, color, blink)| TypeConfig {
                    key: key.into(),
                    label: label.into(),
                    enabled,
                    duration,
                    color: color.into(),
                    sound: false,
                    blink,
                })
                .collect(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into());
        base.join("claude-notifs").join("config.json")
    }

    pub fn load() -> Config {
        let mut cfg: Config = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(Self::from_json)
            .unwrap_or_default();
        cfg.normalize();
        cfg
    }

    /// Chaque type lu est fusionné sur son type par défaut : un champ absent d'une ancienne
    /// configuration (ex. `blink`) prend sa valeur par défaut et non `false`.
    fn from_json(mut v: serde_json::Value) -> Option<Config> {
        let saved = v.get("types").and_then(|t| t.as_array()).cloned().unwrap_or_default();
        let merged: Vec<serde_json::Value> = Config::default()
            .types
            .iter()
            .map(|def| {
                let mut obj = serde_json::to_value(def).unwrap_or_default();
                let user = saved.iter().find(|t| t.get("key").and_then(|k| k.as_str()) == Some(&def.key));
                if let (Some(base), Some(user)) = (obj.as_object_mut(), user.and_then(|u| u.as_object())) {
                    for (k, val) in user {
                        base.insert(k.clone(), val.clone());
                    }
                }
                obj
            })
            .collect();
        v.as_object_mut()?.insert("types".into(), serde_json::Value::Array(merged));
        serde_json::from_value(v).ok()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    /// Remet les types dans l'ordre par défaut, ajoute ceux qui manquent, retire les inconnus.
    pub fn normalize(&mut self) {
        let defaults = Config::default().types;
        let current = std::mem::take(&mut self.types);
        self.types = defaults
            .into_iter()
            .map(|def| current.iter().find(|t| t.key == def.key).cloned().unwrap_or(def))
            .collect();
        self.max_visible = self.max_visible.clamp(1, 10);
    }

    pub fn type_config(&self, key: &str) -> Option<&TypeConfig> {
        self.types.iter().find(|t| t.key == key)
    }
}
