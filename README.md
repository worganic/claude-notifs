# Claude Notifs

Mini application Windows (Rust) qui affiche des notifications quand Claude Code pose une question,
demande une permission, présente un plan, termine un prompt, etc.

## Compiler et lancer

```powershell
cargo build --release
.\target\release\claude-notifs.exe              # lance l'app et ouvre l'administration
.\target\release\claude-notifs.exe --background # lance l'app sans ouvrir l'administration
```

L'application vit dans la zone de notification. Un clic gauche sur l'icône ouvre l'administration.
Un clic droit affiche le menu : Administration, Tester, Ne pas déranger, Quitter.

> Quitte l'application (menu → Quitter) avant de recompiler : sinon l'exe est verrouillé.

## Liaison avec Claude Code

Pour installer la liaison, utilise le bouton **Installer la liaison** de l'administration, ou la ligne de commande :

```powershell
.\target\release\claude-notifs.exe install-hooks    # ajoute les hooks dans ~/.claude/settings.json
.\target\release\claude-notifs.exe uninstall-hooks  # les retire
```

- Seules les entrées dont la commande contient `claude-notifs` sont ajoutées ou retirées. Les autres hooks ne sont pas touchés.
- Une sauvegarde est écrite dans `~/.claude/settings.json.bak-claude-notifs` avant chaque modification.
- Chaque hook exécute `claude-notifs.exe hook <Event>`. Cette commande lit le JSON du hook sur stdin et l'envoie à
  `http://127.0.0.1:47790/event`. Si l'application n'est pas lancée, le hook la démarre en arrière-plan.
- Le hook rend toujours la main en moins de 5 s et sort avec le code 0 : il ne bloque jamais Claude Code.

| Type | Hook Claude Code | Durée par défaut |
|---|---|---|
| Question posée | `PreToolUse` (AskUserQuestion), `Notification` elicitation_dialog | jusqu'au clic |
| Plan à valider | `PreToolUse` (ExitPlanMode) | jusqu'au clic |
| Permission demandée | `Notification` permission_prompt | jusqu'au clic |
| Claude attend ta réponse | `Notification` idle_prompt | 10 s |
| Fin de prompt | `Stop` | 6 s |
| Début de prompt | `UserPromptSubmit` | 3 s |
| Fin de sous-agent / début et fin de session | `SubagentStop`, `SessionStart`, `SessionEnd` | désactivés |

Les notifications qui attendent une action (question, plan, permission, attente) se ferment d'elles-mêmes
au prochain `UserPromptSubmit` ou `Stop` de la même session.

## Administration

- **Position** : haut ou bas, gauche ou droite. Tu choisis aussi le nombre maximal de popups visibles en même temps.
- **Types** : pour chaque type, activation, libellé, durée (0 = jusqu'au clic), couleur, son d'alerte,
  clignotement et bouton Tester.
  - **Son** : joue le son système « Notification » de Windows, réglable dans Paramètres → Son → Plus de paramètres
    de son → Sons. Il est désactivé par défaut, pour ne pas doubler un hook sonore déjà en place.
  - **Clignote** : le fond et l'icône de la popup pulsent tant qu'elle est affichée. Ce réglage est activé par
    défaut pour les questions, les plans et les permissions.
- **Liaison Claude Code** : état, dernier événement reçu, boutons installer / retirer.
- **Général** : démarrage avec Windows (clé `HKCU\...\Run`) et mode Ne pas déranger.

La configuration est enregistrée dans `%APPDATA%\claude-notifs\config.json`.

## Tester sans Claude Code

```powershell
'{"cwd":"E:\\www\\demo","notification_type":"permission_prompt","message":"test"}' |
  .\target\release\claude-notifs.exe hook Notification
```
