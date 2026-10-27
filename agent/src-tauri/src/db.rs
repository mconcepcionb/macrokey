use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use indexmap::IndexMap;
use rusqlite::{params, Connection, OptionalExtension};

use crate::config::{
    Action, Binding, ConfigFile, Context, GestureMap, Input, Layer, Matcher, Rule, StateSettings,
};

const SEED_YAML: &str = include_str!("../../../config/agent.draft.yaml");

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS layers (
    id       TEXT PRIMARY KEY,
    label    TEXT,
    color    TEXT,
    position INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS contexts (
    id            TEXT PRIMARY KEY,
    label         TEXT,
    default_layer TEXT NOT NULL,
    position      INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS context_layers (
    context_id TEXT NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
    layer_id   TEXT NOT NULL REFERENCES layers(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (context_id, layer_id)
);
CREATE TABLE IF NOT EXISTS bindings (
    layer_id TEXT NOT NULL REFERENCES layers(id) ON DELETE CASCADE,
    input    TEXT NOT NULL,
    gesture  TEXT NOT NULL,
    action   TEXT NOT NULL,
    PRIMARY KEY (layer_id, input, gesture)
);
CREATE TABLE IF NOT EXISTS rules (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    position   INTEGER NOT NULL DEFAULT 0,
    process    TEXT,
    title      TEXT,
    class      TEXT,
    context_id TEXT NOT NULL REFERENCES contexts(id) ON DELETE CASCADE
);
"#;

/// Bump this and add a migration block in `initialize` for every schema change.
const SCHEMA_VERSION: i64 = 2;

// ---------------------------------------------------------------------------
// location
// ---------------------------------------------------------------------------

fn arg_value(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

pub fn db_path() -> PathBuf {
    if let Some(path) = arg_value("--db") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("MACROKEY_DB") {
        return PathBuf::from(path);
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Path::new(&appdata).join("macrokey").join("macrokey.db");
    }
    PathBuf::from("macrokey.db")
}

pub fn seed_override() -> Option<PathBuf> {
    arg_value("--seed").map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// open / initialize
// ---------------------------------------------------------------------------

fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let connection =
        Connection::open(path).with_context(|| format!("open database {}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(3))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(connection)
}

fn backup_database(connection: &Connection, path: &Path, schema: i64) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "macrokey.db".into());

    let mut backup = path.with_file_name(format!("{file_name}.bak-v{schema}-{stamp}"));
    let mut suffix = 0;
    while backup.exists() {
        suffix += 1;
        backup = path.with_file_name(format!("{file_name}.bak-v{schema}-{stamp}-{suffix}"));
    }

    // VACUUM INTO produces a consistent copy, including committed WAL pages.
    let escaped = backup.to_string_lossy().replace('\'', "''");
    connection
        .execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .with_context(|| format!("backup database to {}", backup.display()))?;
    Ok(backup)
}

fn initialize(connection: &mut Connection, path: &Path, seed: Option<&Path>) -> Result<()> {
    connection.execute_batch(SCHEMA)?;

    let mut schema: i64 = connection
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let had_data: i64 =
        connection.query_row("SELECT COUNT(*) FROM contexts", [], |row| row.get(0))?;

    if schema == 0 {
        // Early databases did not write the schema key; treat them as v1.
        schema = 1;
        connection.execute(
            "INSERT INTO meta(key, value) VALUES('schema', '1')
             ON CONFLICT(key) DO NOTHING",
            [],
        )?;
    }

    if schema > SCHEMA_VERSION {
        anyhow::bail!(
            "database schema v{schema} is newer than this build (v{SCHEMA_VERSION}); update macrokey"
        );
    }

    if schema < SCHEMA_VERSION {
        if had_data > 0 {
            let backup = backup_database(connection, path, schema)?;
            crate::log_info!("[db] backed up schema v{schema} to {}", backup.display());
        }

        let transaction = connection.transaction()?;
        if schema < 2 {
            transaction.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_rules_context ON rules(context_id);",
            )?;
        }
        transaction.execute(
            "INSERT INTO meta(key, value) VALUES('schema', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
        transaction.commit()?;
        if had_data > 0 {
            crate::log_info!("[db] migrated schema v{schema} -> v{SCHEMA_VERSION}");
        } else {
            crate::log_info!("[db] initialized schema v{SCHEMA_VERSION}");
        }
    }

    connection.execute(
        "INSERT INTO meta(key, value) VALUES('revision', '0')
         ON CONFLICT(key) DO NOTHING",
        [],
    )?;

    let contexts: i64 =
        connection.query_row("SELECT COUNT(*) FROM contexts", [], |row| row.get(0))?;
    if contexts == 0 {
        let file = match seed {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("read seed {}", path.display()))?;
                serde_yaml::from_str::<ConfigFile>(&text)
                    .with_context(|| format!("parse seed {}", path.display()))?
            }
            None => serde_yaml::from_str(SEED_YAML).context("parse embedded seed")?,
        };
        let transaction = connection.transaction()?;
        write_config(&transaction, &file)?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        crate::log_info!(
            "[db] seeded database from {}",
            seed.map(|p| p.display().to_string())
                .unwrap_or_else(|| "embedded defaults".into())
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// read
// ---------------------------------------------------------------------------

pub fn load_config(path: &Path, seed: Option<&Path>) -> Result<ConfigFile> {
    let mut connection = open(path)?;
    initialize(&mut connection, path, seed)?;
    read_config(&connection)
}

fn read_config(connection: &Connection) -> Result<ConfigFile> {
    let stored: StateSettings = connection
        .query_row("SELECT json FROM settings WHERE id = 1", [], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .context("parse settings json")?
        .unwrap_or_default();

    let mut layers: IndexMap<String, Layer> = IndexMap::new();
    {
        let mut statement =
            connection.prepare("SELECT id, label, color FROM layers ORDER BY position, id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            let (id, label, color) = row?;
            layers.insert(
                id,
                Layer {
                    label,
                    color,
                    bindings: HashMap::new(),
                },
            );
        }
    }

    let mut contexts: IndexMap<String, Context> = IndexMap::new();
    {
        let mut statement = connection
            .prepare("SELECT id, label, default_layer FROM contexts ORDER BY position, id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, label, default_layer) = row?;
            contexts.insert(
                id,
                Context {
                    label,
                    default_layer,
                    layers: Vec::new(),
                },
            );
        }
    }

    {
        let mut statement = connection.prepare(
            "SELECT context_id, layer_id FROM context_layers ORDER BY context_id, position",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (context_id, layer_id) = row?;
            if let Some(context) = contexts.get_mut(&context_id) {
                context.layers.push(layer_id);
            }
        }
    }

    let mut gestures: HashMap<(String, Input), GestureMap> = HashMap::new();
    {
        let mut statement =
            connection.prepare("SELECT layer_id, input, gesture, action FROM bindings")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (layer_id, input_name, gesture, action_json) = row?;
            let Some(input) = Input::from_name(&input_name) else {
                continue;
            };
            let action: Action = serde_json::from_str(&action_json)
                .with_context(|| format!("parse action for {layer_id}/{input_name}/{gesture}"))?;
            let entry = gestures.entry((layer_id, input)).or_default();
            match gesture.as_str() {
                "tap" => entry.tap = Some(action),
                "double" => entry.double = Some(action),
                "hold" => entry.hold = Some(action),
                _ => {}
            }
        }
    }
    for ((layer_id, input), map) in gestures {
        let Some(layer) = layers.get_mut(&layer_id) else {
            continue;
        };
        let only_tap = map.tap.is_some() && map.double.is_none() && map.hold.is_none();
        if only_tap {
            layer
                .bindings
                .insert(input, Binding::Single(map.tap.unwrap()));
        } else if map.tap.is_some() || map.double.is_some() || map.hold.is_some() {
            layer.bindings.insert(input, Binding::Gestures(map));
        }
    }

    let mut rules = Vec::new();
    {
        let mut statement = connection
            .prepare("SELECT process, title, class, context_id FROM rules ORDER BY position, id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (process, title, class, context) = row?;
            rules.push(Rule {
                matcher: Matcher {
                    process,
                    title,
                    class,
                },
                context,
            });
        }
    }

    Ok(ConfigFile {
        version: 3,
        pad: stored.pad,
        settings: stored.settings,
        layers,
        contexts,
        rules,
        default_context: stored.default_context,
    })
}

// ---------------------------------------------------------------------------
// write
// ---------------------------------------------------------------------------

fn write_config(transaction: &rusqlite::Transaction<'_>, file: &ConfigFile) -> Result<()> {
    transaction.execute("DELETE FROM bindings", [])?;
    transaction.execute("DELETE FROM context_layers", [])?;
    transaction.execute("DELETE FROM rules", [])?;
    transaction.execute("DELETE FROM contexts", [])?;
    transaction.execute("DELETE FROM layers", [])?;

    for (position, (id, layer)) in file.layers.iter().enumerate() {
        transaction.execute(
            "INSERT INTO layers(id, label, color, position) VALUES(?1, ?2, ?3, ?4)",
            params![id, layer.label, layer.color, position as i64],
        )?;
    }

    for (position, (id, context)) in file.contexts.iter().enumerate() {
        transaction.execute(
            "INSERT INTO contexts(id, label, default_layer, position)
             VALUES(?1, ?2, ?3, ?4)",
            params![id, context.label, context.default_layer, position as i64],
        )?;
        for (layer_position, layer_id) in context.layers.iter().enumerate() {
            transaction.execute(
                "INSERT OR IGNORE INTO context_layers(context_id, layer_id, position)
                 VALUES(?1, ?2, ?3)",
                params![id, layer_id, layer_position as i64],
            )?;
        }
    }

    for (layer_id, layer) in &file.layers {
        for (input, binding) in &layer.bindings {
            let mut rows: Vec<(&str, &Action)> = Vec::new();
            match binding {
                Binding::Single(action) => rows.push(("tap", action)),
                Binding::Gestures(map) => {
                    if let Some(action) = &map.tap {
                        rows.push(("tap", action));
                    }
                    if let Some(action) = &map.double {
                        rows.push(("double", action));
                    }
                    if let Some(action) = &map.hold {
                        rows.push(("hold", action));
                    }
                }
            }
            for (gesture, action) in rows {
                transaction.execute(
                    "INSERT INTO bindings(layer_id, input, gesture, action)
                     VALUES(?1, ?2, ?3, ?4)",
                    params![
                        layer_id,
                        input.key(),
                        gesture,
                        serde_json::to_string(action)?
                    ],
                )?;
            }
        }
    }

    for (position, rule) in file.rules.iter().enumerate() {
        transaction.execute(
            "INSERT INTO rules(position, process, title, class, context_id)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                position as i64,
                rule.matcher.process,
                rule.matcher.title,
                rule.matcher.class,
                rule.context
            ],
        )?;
    }

    let stored = StateSettings {
        pad: file.pad.clone(),
        settings: file.settings.clone(),
        default_context: file.default_context.clone(),
    };
    transaction.execute(
        "INSERT INTO settings(id, json) VALUES(1, ?1)
         ON CONFLICT(id) DO UPDATE SET json = excluded.json",
        params![serde_json::to_string(&stored)?],
    )?;

    Ok(())
}

fn bump_revision(connection: &Connection) -> Result<()> {
    let revision: i64 = connection
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'revision'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    connection.execute(
        "INSERT INTO meta(key, value) VALUES('revision', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![(revision + 1).to_string()],
    )?;
    Ok(())
}

pub fn save_config(path: &Path, file: &ConfigFile) -> Result<i64> {
    // Validate before touching the database.
    crate::config::compile(file.clone(), path.to_path_buf())?;

    let mut connection = open(path)?;
    initialize(&mut connection, path, None)?;
    let transaction = connection.transaction()?;
    write_config(&transaction, file)?;
    bump_revision(&transaction)?;
    let revision: i64 = transaction.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'revision'",
        [],
        |row| row.get(0),
    )?;
    transaction.commit()?;
    Ok(revision)
}

pub fn reset(path: &Path) -> Result<i64> {
    let file: ConfigFile = serde_yaml::from_str(SEED_YAML).context("parse embedded seed")?;
    crate::config::compile(file.clone(), path.to_path_buf())?;

    let mut connection = open(path)?;
    initialize(&mut connection, path, None)?;
    let transaction = connection.transaction()?;
    write_config(&transaction, &file)?;
    bump_revision(&transaction)?;
    let revision: i64 = transaction.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'revision'",
        [],
        |row| row.get(0),
    )?;
    transaction.commit()?;
    Ok(revision)
}

pub fn revision(path: &Path) -> Result<i64> {
    let connection = Connection::open(path)?;
    let value: Option<String> = connection
        .query_row("SELECT value FROM meta WHERE key = 'revision'", [], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(value.and_then(|value| value.parse().ok()).unwrap_or(0))
}

pub fn export_yaml(path: &Path, destination: &Path) -> Result<()> {
    let file = load_config(path, None)?;
    let yaml = crate::config::to_yaml(&file)?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(destination, yaml)
        .with_context(|| format!("write {}", destination.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Input;

    #[test]
    fn sqlite_round_trip() {
        let directory = std::env::temp_dir().join(format!(
            "macrokey-db-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("test.db");

        let file: ConfigFile =
            serde_yaml::from_str(include_str!("../../../config/agent.draft.yaml")).unwrap();
        save_config(&path, &file).unwrap();

        let loaded = load_config(&path, None).unwrap();
        assert_eq!(loaded.contexts.len(), file.contexts.len());
        assert_eq!(loaded.layers.len(), file.layers.len());
        assert_eq!(loaded.rules.len(), file.rules.len());
        assert_eq!(
            loaded.contexts["vscode"].layers,
            file.contexts["vscode"].layers
        );
        assert_eq!(
            loaded.contexts["vscode"].default_layer,
            file.contexts["vscode"].default_layer
        );

        let edit = &loaded.layers["edit"];
        let k3 = edit
            .bindings
            .get(&Input::K3)
            .expect("k3 binding survives sqlite");
        assert!(k3.tap().is_some());
        assert!(k3.double().is_some());
        assert!(k3.hold().is_some());

        let revision_before = revision(&path).unwrap();
        save_config(&path, &loaded).unwrap();
        assert_eq!(revision(&path).unwrap(), revision_before + 1);

        let exported = directory.join("export.yaml");
        export_yaml(&path, &exported).unwrap();
        assert!(exported.exists());

        let _ = std::fs::remove_dir_all(&directory);
    }

    fn temp_directory(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "macrokey-db-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn migrates_v1_schema_and_writes_backup() {
        let directory = temp_directory("migration");
        let path = directory.join("test.db");
        let file: ConfigFile = serde_yaml::from_str(SEED_YAML).unwrap();
        save_config(&path, &file).unwrap();

        // Simulate a v0.1 database: schema 1, before the rules index existed.
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute("UPDATE meta SET value = '1' WHERE key = 'schema'", [])
                .unwrap();
            connection
                .execute("DROP INDEX IF EXISTS idx_rules_context", [])
                .unwrap();
        }

        let loaded = load_config(&path, None).unwrap();
        assert_eq!(loaded.contexts.len(), file.contexts.len());
        assert_eq!(loaded.rules.len(), file.rules.len());

        let connection = Connection::open(&path).unwrap();
        let schema: String = connection
            .query_row("SELECT value FROM meta WHERE key = 'schema'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(schema, SCHEMA_VERSION.to_string());
        let index: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_rules_context'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index, 1);
        drop(connection);

        let backups: Vec<std::path::PathBuf> = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.file_name()?.to_string_lossy().contains(".bak-v1-") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(backups.len(), 1, "expected one schema-v1 backup");
        let backup = Connection::open(&backups[0]).unwrap();
        let backup_schema: String = backup
            .query_row("SELECT value FROM meta WHERE key = 'schema'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(backup_schema, "1");
        let backup_contexts: i64 = backup
            .query_row("SELECT COUNT(*) FROM contexts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(backup_contexts, file.contexts.len() as i64);
        drop(backup);

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn refuses_a_newer_schema() {
        let directory = temp_directory("newer-schema");
        let path = directory.join("test.db");
        let file: ConfigFile = serde_yaml::from_str(SEED_YAML).unwrap();
        save_config(&path, &file).unwrap();

        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute("UPDATE meta SET value = '99' WHERE key = 'schema'", [])
                .unwrap();
        }

        let error = load_config(&path, None).expect_err("a newer schema must be refused");
        assert!(format!("{error:#}").contains("newer"));

        let _ = std::fs::remove_dir_all(&directory);
    }
}
