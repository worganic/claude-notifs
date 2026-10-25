//! Traduction d'un événement de hook Claude Code en notification.

use serde_json::Value;

pub struct Notif {
    pub kind: String,
    pub project: String,
    pub message: String,
    pub session_id: String,
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Raccourcit `s` à `max` caractères (sur une seule ligne) en ajoutant « … ».
pub fn truncate(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let cut: String = flat.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}

fn project_name(cwd: &str) -> String {
    cwd.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_string()
}

/// Retourne `None` pour les événements qui ne doivent pas produire de notification.
pub fn map(event: &str, p: &Value) -> Option<Notif> {
    let (kind, message) = match event {
        "UserPromptSubmit" => ("prompt_start", truncate(str_field(p, "prompt"), 160)),
        "Stop" => {
            let last = truncate(str_field(p, "last_assistant_message"), 160);
            let msg = if last.is_empty() { "Claude a terminé de répondre".to_string() } else { last };
            ("prompt_end", msg)
        }
        "SubagentStop" => ("subagent_end", "Un sous-agent a terminé".to_string()),
        "SessionStart" => ("session_start", format!("Session démarrée ({})", str_field(p, "source"))),
        "SessionEnd" => ("session_end", "Session terminée".to_string()),
        "Notification" => {
            let msg = truncate(str_field(p, "message"), 160);
            let kind = match str_field(p, "notification_type") {
                "permission_prompt" => "permission",
                "idle_prompt" => "idle",
                "elicitation_dialog" => "question",
                _ => "notification",
            };
            (kind, msg)
        }
        "PreToolUse" => match str_field(p, "tool_name") {
            "AskUserQuestion" => {
                let q = p
                    .pointer("/tool_input/questions/0/question")
                    .and_then(Value::as_str)
                    .unwrap_or("Claude a une question pour toi");
                ("question", truncate(q, 160))
            }
            "ExitPlanMode" => ("plan", "Un plan attend ta validation".to_string()),
            _ => return None,
        },
        _ => return None,
    };
    Some(Notif {
        kind: kind.to_string(),
        project: project_name(str_field(p, "cwd")),
        message,
        session_id: str_field(p, "session_id").to_string(),
    })
}
