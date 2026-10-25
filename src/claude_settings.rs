//! Installation / désinstallation des hooks claude-notifs dans ~/.claude/settings.json.
//! Seules nos entrées (commande contenant MARKER) sont ajoutées ou retirées ; le reste du
//! fichier est conservé tel quel (ordre des clés préservé).

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

const MARKER: &str = "claude-notifs";

/// (événement, matcher éventuel)
const HOOKS: &[(&str, Option<&str>)] = &[
    ("UserPromptSubmit", None),
    ("Stop", None),
    ("Notification", None),
    ("PreToolUse", Some("AskUserQuestion|ExitPlanMode")),
    ("SubagentStop", None),
    ("SessionStart", None),
    ("SessionEnd", None),
];

pub fn settings_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| ".".into());
    home.join(".claude").join("settings.json")
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(MARKER))
}

fn read() -> Result<Value, String> {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(s) if s.trim().is_empty() => Ok(json!({})),
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("settings.json invalide : {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("lecture de {} impossible : {e}", path.display())),
    }
}

fn write(root: &Value) -> Result<(), String> {
    let path = settings_path();
    if path.exists() {
        let backup = path.with_file_name("settings.json.bak-claude-notifs");
        std::fs::copy(&path, &backup).map_err(|e| format!("sauvegarde impossible : {e}"))?;
    } else if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string_pretty(root).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| format!("écriture impossible : {e}"))
}

/// Retire nos hooks de `hooks` ; supprime les groupes et événements que cela vide.
fn strip_ours(hooks: &mut Map<String, Value>) -> bool {
    let mut changed = false;
    for groups in hooks.values_mut() {
        let Some(arr) = groups.as_array_mut() else { continue };
        for group in arr.iter_mut() {
            if let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = list.len();
                list.retain(|h| !is_ours(h));
                changed |= list.len() != before;
            }
        }
        arr.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|l| !l.is_empty()));
    }
    hooks.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
    changed
}

pub fn is_installed() -> bool {
    let Ok(root) = read() else { return false };
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else { return false };
    HOOKS.iter().all(|(event, _)| {
        hooks.get(*event).and_then(Value::as_array).is_some_and(|groups| {
            groups.iter().any(|g| {
                g.get("hooks").and_then(Value::as_array).is_some_and(|l| l.iter().any(is_ours))
            })
        })
    })
}

pub fn install(exe: &Path) -> Result<String, String> {
    let mut root = read()?;
    let obj = root.as_object_mut().ok_or("settings.json n'est pas un objet JSON")?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("la clé \"hooks\" n'est pas un objet")?;
    strip_ours(hooks);

    // Slashs avant : la commande fonctionne aussi bien sous cmd que sous Git Bash.
    let exe = exe.display().to_string().replace('\\', "/");
    for (event, matcher) in HOOKS {
        let mut group = Map::new();
        if let Some(m) = matcher {
            group.insert("matcher".into(), json!(m));
        }
        group.insert(
            "hooks".into(),
            json!([{ "type": "command", "command": format!("\"{exe}\" hook {event}"), "timeout": 10 }]),
        );
        let groups = hooks.entry(*event).or_insert_with(|| json!([]));
        if let Some(arr) = groups.as_array_mut() {
            arr.push(Value::Object(group));
        }
    }
    write(&root)?;
    Ok(format!("Hooks installés dans {}", settings_path().display()))
}

pub fn uninstall() -> Result<String, String> {
    let mut root = read()?;
    let removed = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .map(strip_ours)
        .unwrap_or(false);
    if !removed {
        return Ok("Aucun hook claude-notifs à retirer".into());
    }
    write(&root)?;
    Ok(format!("Hooks retirés de {}", settings_path().display()))
}
