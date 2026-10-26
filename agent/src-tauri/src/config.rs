use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::actions::key_to_vk;
use crate::context::FocusInfo;

// ---------------------------------------------------------------------------
// YAML / JSON model
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct PadCfg {
    pub vid: u32,
    pub pid: u32,
    pub interface: String,
    pub inputs: HashMap<String, String>,
}

impl Default for PadCfg {
    fn default() -> Self {
        let mut inputs = HashMap::new();
        inputs.insert("k1".into(), "F13".into());
        inputs.insert("k2".into(), "F14".into());
        inputs.insert("k3".into(), "F15".into());
        inputs.insert("knob_press".into(), "F16".into());
        inputs.insert("knob_ccw".into(), "F17".into());
        inputs.insert("knob_cw".into(), "F18".into());
        Self {
            vid: 0x1189,
            pid: 0x8890,
            interface: "MI_00".into(),
            inputs,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Settings {
    pub osd: OsdCfg,
    pub gestures: GestureCfg,
    pub knob: KnobCfg,
}

/// The part of the state that is not a table of its own.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct StateSettings {
    pub pad: PadCfg,
    pub settings: Settings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_context: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct OsdCfg {
    pub enabled: bool,
    pub duration_ms: u64,
    pub monitor: String,
    pub anchor: String,
    pub show_on_focus: bool,
    pub show_on_press: bool,
}

impl Default for OsdCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_ms: 1500,
            monitor: "primary".into(),
            anchor: "bottom-center".into(),
            show_on_focus: false,
            show_on_press: true,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct GestureCfg {
    pub double_tap_ms: u64,
    #[serde(alias = "long_press_ms")]
    pub hold_ms: u64,
}

impl Default for GestureCfg {
    fn default() -> Self {
        Self {
            double_tap_ms: 260,
            hold_ms: 420,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct KnobCfg {
    pub acceleration: bool,
    pub window_ms: u64,
    pub multiplier: Vec<u32>,
}

impl Default for KnobCfg {
    fn default() -> Self {
        Self {
            acceleration: true,
            window_ms: 120,
            multiplier: vec![1, 1, 2, 4, 8],
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    K1,
    K2,
    K3,
    KnobPress,
    KnobCcw,
    KnobCw,
}

impl Input {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "k1" => Some(Self::K1),
            "k2" => Some(Self::K2),
            "k3" => Some(Self::K3),
            "knob_press" => Some(Self::KnobPress),
            "knob_ccw" => Some(Self::KnobCcw),
            "knob_cw" => Some(Self::KnobCw),
            _ => None,
        }
    }

    pub fn is_rotation(self) -> bool {
        matches!(self, Self::KnobCcw | Self::KnobCw)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::K1 => "k1",
            Self::K2 => "k2",
            Self::K3 => "k3",
            Self::KnobPress => "knob_press",
            Self::KnobCcw => "knob_ccw",
            Self::KnobCw => "knob_cw",
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayerMode {
    #[default]
    Switch,
    Momentary,
    Oneshot,
    Next,
    Prev,
    Back,
}

impl LayerMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Switch => "switch",
            Self::Momentary => "while held",
            Self::Oneshot => "next key",
            Self::Next => "next layer",
            Self::Prev => "previous layer",
            Self::Back => "context default",
        }
    }

    pub fn needs_name(self) -> bool {
        matches!(self, Self::Switch | Self::Momentary | Self::Oneshot)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum SequenceStep {
    Keys { keys: Vec<String> },
    Text { text: String },
    Delay { delay: u64 },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Hotkey {
        keys: Vec<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        repeat: bool,
    },
    Sequence {
        steps: Vec<SequenceStep>,
    },
    Media {
        key: String,
    },
    Wheel {
        delta: i32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
    },
    Text {
        value: String,
    },
    Launch {
        target: String,
    },
    Url {
        target: String,
    },
    Script {
        cmd: String,
    },
    Window {
        #[serde(rename = "do")]
        what: String,
    },
    Layer {
        #[serde(default)]
        mode: LayerMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Peek,
    #[serde(rename = "none")]
    Nothing,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Action {
    pub fn label(&self) -> String {
        match self {
            Self::Hotkey { keys, .. } => keys
                .iter()
                .map(|k| pretty_key(k))
                .collect::<Vec<_>>()
                .join("+"),
            Self::Sequence { steps } => format!("Sequence ×{}", steps.len()),
            Self::Media { key } => pretty_key(key),
            Self::Wheel { delta, .. } => {
                if *delta < 0 {
                    "Wheel ↓".into()
                } else {
                    "Wheel ↑".into()
                }
            }
            Self::Text { value } => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    "Text".into()
                } else {
                    format!("\u{201c}{}\u{201d}", truncate(trimmed, 14))
                }
            }
            Self::Launch { target } => format!("Open {}", truncate(target, 16)),
            Self::Url { target } => format!("URL {}", truncate(target, 16)),
            Self::Script { .. } => "Script".into(),
            Self::Window { what } => format!("Window {what}"),
            Self::Layer { mode, name } => match mode {
                LayerMode::Next | LayerMode::Prev | LayerMode::Back => {
                    format!("Layer: {}", mode.label())
                }
                _ => match name {
                    Some(name) => format!("Layer: {name} ({})", mode.label()),
                    None => "Layer".into(),
                },
            },
            Self::Peek => "Peek legend".into(),
            Self::Nothing => "—".into(),
        }
    }

    fn validate(&self, layers: &IndexMap<String, Layer>) -> Result<()> {
        match self {
            Self::Hotkey { keys, .. } => {
                if keys.is_empty() {
                    bail!("hotkey has no keys");
                }
                for key in keys {
                    if key_to_vk(key).is_none() {
                        bail!("unknown key {key:?}");
                    }
                }
            }
            Self::Sequence { steps } => {
                if steps.is_empty() {
                    bail!("sequence has no steps");
                }
                for step in steps {
                    match step {
                        SequenceStep::Keys { keys } => {
                            if keys.is_empty() {
                                bail!("sequence step has no keys");
                            }
                            for key in keys {
                                if key_to_vk(key).is_none() {
                                    bail!("unknown key {key:?}");
                                }
                            }
                        }
                        SequenceStep::Delay { delay } => {
                            if *delay > 10_000 {
                                bail!("delay {delay} ms is too long (max 10000)");
                            }
                        }
                        SequenceStep::Text { .. } => {}
                    }
                }
            }
            Self::Media { key } => {
                if key_to_vk(key).is_none() {
                    bail!("unknown media key {key:?}");
                }
            }
            Self::Wheel { modifiers, .. } => {
                for key in modifiers {
                    if key_to_vk(key).is_none() {
                        bail!("unknown wheel modifier {key:?}");
                    }
                }
            }
            Self::Layer { mode, name } => {
                if mode.needs_name() {
                    let Some(name) = name else {
                        bail!("layer action needs a layer");
                    };
                    if !layers.contains_key(name) {
                        bail!("unknown layer {name:?}");
                    }
                }
            }
            Self::Window { what } => {
                let what = what.to_ascii_lowercase();
                if !matches!(what.as_str(), "minimize" | "maximize" | "restore" | "close") {
                    bail!("unknown window action {what:?}");
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn pretty_key(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "ctrl" | "rctrl" => "Ctrl".into(),
        "shift" | "rshift" => "Shift".into(),
        "alt" | "ralt" | "opt" | "ropt" => "Alt".into(),
        "win" | "cmd" | "rwin" | "rcmd" => "Win".into(),
        "enter" => "Enter".into(),
        "esc" => "Esc".into(),
        "space" => "Space".into(),
        "tab" => "Tab".into(),
        "grave" => "`".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "volumeup" | "volume_up" | "volup" => "Vol +".into(),
        "volumedown" | "volume_down" | "voldown" => "Vol −".into(),
        "play" | "playpause" | "play_pause" => "Play/Pause".into(),
        "next" => "Next".into(),
        "previous" | "prev" => "Previous".into(),
        "stop" => "Stop".into(),
        "mute" => "Mute".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct GestureMap {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tap: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub double: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<Action>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Binding {
    Single(Action),
    Gestures(GestureMap),
}

impl Binding {
    pub fn tap(&self) -> Option<&Action> {
        match self {
            Self::Single(action) => Some(action),
            Self::Gestures(map) => map.tap.as_ref(),
        }
    }
    pub fn double(&self) -> Option<&Action> {
        match self {
            Self::Single(_) => None,
            Self::Gestures(map) => map.double.as_ref(),
        }
    }
    pub fn hold(&self) -> Option<&Action> {
        match self {
            Self::Single(_) => None,
            Self::Gestures(map) => map.hold.as_ref(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.tap().is_none() && self.double().is_none() && self.hold().is_none()
    }
}

impl Default for Binding {
    fn default() -> Self {
        Self::Gestures(GestureMap::default())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Layer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub bindings: HashMap<Input, Binding>,
}

impl Layer {
    pub fn title(&self, fallback: &str) -> String {
        self.label.clone().unwrap_or_else(|| fallback.to_string())
    }
    pub fn color(&self) -> String {
        self.color.clone().unwrap_or_else(|| "#7dd3fc".into())
    }
    pub fn binding(&self, input: Input) -> Option<&Binding> {
        self.bindings.get(&input).filter(|b| !b.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Contexts
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Context {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub default_layer: String,
    pub layers: Vec<String>,
}

impl Context {
    pub fn title(&self, fallback: &str) -> String {
        self.label.clone().unwrap_or_else(|| fallback.to_string())
    }
    pub fn contains(&self, layer: &str) -> bool {
        self.layers.iter().any(|entry| entry == layer)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Matcher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Rule {
    #[serde(rename = "match")]
    pub matcher: Matcher,
    pub context: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ConfigFile {
    pub version: u32,
    pub pad: PadCfg,
    pub settings: Settings,
    pub layers: IndexMap<String, Layer>,
    pub contexts: IndexMap<String, Context>,
    pub rules: Vec<Rule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_context: Option<String>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            version: 3,
            pad: PadCfg::default(),
            settings: Settings::default(),
            layers: IndexMap::new(),
            contexts: IndexMap::new(),
            rules: Vec::new(),
            default_context: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime model
// ---------------------------------------------------------------------------

pub struct CompiledRule {
    pub process: Option<String>,
    pub title: Option<Regex>,
    pub class: Option<String>,
    pub context: String,
}

impl CompiledRule {
    fn matches(&self, focus: &FocusInfo) -> bool {
        if let Some(process) = &self.process {
            if !focus.exe.eq_ignore_ascii_case(process) {
                return false;
            }
        }
        if let Some(title) = &self.title {
            if !title.is_match(&focus.title) {
                return false;
            }
        }
        if let Some(class) = &self.class {
            if !focus.class.eq_ignore_ascii_case(class) {
                return false;
            }
        }
        true
    }
}

pub struct Runtime {
    pub layers: IndexMap<String, Layer>,
    pub contexts: IndexMap<String, Context>,
    pub default_context: String,
    pub rules: Vec<CompiledRule>,
    pub settings: Settings,
    pub inputs: HashMap<u16, Input>,
    pub vid: u32,
    pub pid: u32,
    pub path: PathBuf,
}

impl Runtime {
    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.get(name)
    }

    pub fn context(&self, name: &str) -> Option<&Context> {
        self.contexts.get(name)
    }

    pub fn context_label(&self, name: &str) -> String {
        self.context(name)
            .map(|context| context.title(name))
            .unwrap_or_else(|| name.to_string())
    }

    pub fn context_layers(&self, name: &str) -> Vec<String> {
        self.context(name)
            .map(|context| context.layers.clone())
            .unwrap_or_default()
    }

    pub fn context_default_layer(&self, name: &str) -> String {
        self.context(name)
            .map(|context| context.default_layer.clone())
            .unwrap_or_default()
    }

    pub fn binding(&self, layer: &str, input: Input) -> Option<&Binding> {
        self.layer(layer).and_then(|l| l.binding(input))
    }

    pub fn resolve_context(&self, focus: &FocusInfo) -> String {
        for rule in &self.rules {
            if rule.matches(focus) && self.contexts.contains_key(&rule.context) {
                return rule.context.clone();
            }
        }
        self.default_context.clone()
    }
}

// ---------------------------------------------------------------------------
// Loading / validation
// ---------------------------------------------------------------------------

pub fn load_runtime() -> Result<Runtime> {
    let path = crate::db::db_path();
    let seed = crate::db::seed_override();
    let file = crate::db::load_config(&path, seed.as_deref())?;
    compile(file, path)
}

pub fn to_yaml(file: &ConfigFile) -> Result<String> {
    Ok(serde_yaml::to_string(file)?)
}

fn normalize_contexts(file: &mut ConfigFile) -> Result<()> {
    if file.contexts.is_empty() {
        bail!("config has no contexts");
    }
    for (name, context) in file.contexts.iter_mut() {
        if context.layers.is_empty() {
            if context.default_layer.is_empty() {
                bail!("context {name:?} has no layers");
            }
            context.layers.push(context.default_layer.clone());
        }
        if context.default_layer.is_empty() {
            context.default_layer = context.layers[0].clone();
        }
        if !context.contains(&context.default_layer) {
            context.layers.insert(0, context.default_layer.clone());
        }
        for layer in &context.layers {
            if !file.layers.contains_key(layer) {
                bail!("context {name:?} references unknown layer {layer:?}");
            }
        }
    }
    Ok(())
}

pub fn compile(mut file: ConfigFile, path: PathBuf) -> Result<Runtime> {
    if file.layers.is_empty() {
        bail!("config has no layers");
    }
    normalize_contexts(&mut file)?;

    let mut rules = Vec::new();
    for rule in &file.rules {
        if !file.contexts.contains_key(&rule.context) {
            bail!("rule points at unknown context {:?}", rule.context);
        }
        let process = rule
            .matcher
            .process
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let title_pattern = rule
            .matcher
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let class = rule
            .matcher
            .class
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if process.is_none() && title_pattern.is_none() && class.is_none() {
            bail!(
                "rule for context {:?} needs a process or a window title",
                rule.context
            );
        }

        let title = match title_pattern {
            Some(pattern) => Some(
                Regex::new(pattern).with_context(|| format!("invalid title regex {pattern:?}"))?,
            ),
            None => None,
        };
        rules.push(CompiledRule {
            process,
            title,
            class,
            context: rule.context.clone(),
        });
    }

    let default_context = file
        .default_context
        .clone()
        .filter(|name| file.contexts.contains_key(name))
        .or_else(|| {
            if file.contexts.contains_key("general") {
                Some("general".to_string())
            } else {
                file.contexts.keys().next().cloned()
            }
        })
        .ok_or_else(|| anyhow::anyhow!("no usable default context"))?;

    for (layer_name, layer) in &file.layers {
        for (input, binding) in &layer.bindings {
            for (gesture, action) in [
                ("tap", binding.tap()),
                ("double", binding.double()),
                ("hold", binding.hold()),
            ] {
                if let Some(action) = action {
                    action
                        .validate(&file.layers)
                        .with_context(|| format!("{layer_name}/{input:?}/{gesture}"))?;
                }
            }
        }
    }

    let mut inputs = HashMap::new();
    for (name, key) in &file.pad.inputs {
        let Some(input) = Input::from_name(name) else {
            crate::log_error!("[config] ignoring unknown input {name:?}");
            continue;
        };
        match key_to_vk(key) {
            Some(vk) => {
                inputs.insert(vk, input);
            }
            None => crate::log_error!("[config] cannot map input {name:?} to key {key:?}"),
        }
    }

    crate::log_info!(
        "[config] {} contexts, {} layers, {} rules, default context {:?}, {} inputs",
        file.contexts.len(),
        file.layers.len(),
        rules.len(),
        default_context,
        inputs.len()
    );

    Ok(Runtime {
        layers: file.layers,
        contexts: file.contexts,
        default_context,
        rules,
        settings: file.settings,
        inputs,
        vid: file.pad.vid,
        pid: file.pad.pid,
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(yaml: &str) {
        let file: ConfigFile = serde_yaml::from_str(yaml).expect("parse yaml");
        let json = serde_json::to_value(&file).expect("to json");
        let back: ConfigFile = serde_json::from_value(json).expect("from json");
        let yaml2 = to_yaml(&back).expect("to yaml");
        let again: ConfigFile = serde_yaml::from_str(&yaml2).expect("reparse yaml");
        compile(again, PathBuf::from("test.yaml")).expect("compile");
    }

    #[test]
    fn draft_round_trips_through_json() {
        round_trip(include_str!("../../../config/agent.draft.yaml"));
    }

    #[test]
    fn test_config_round_trips_through_json() {
        round_trip(include_str!("../../../config/agent.test.yaml"));
    }

    fn draft() -> ConfigFile {
        serde_yaml::from_str(include_str!("../../../config/agent.draft.yaml")).expect("parse draft")
    }

    fn focus(exe: &str, title: &str) -> crate::context::FocusInfo {
        crate::context::FocusInfo {
            exe: exe.to_string(),
            title: title.to_string(),
            class: "TestWindow".to_string(),
        }
    }

    #[test]
    fn rules_are_first_match_case_insensitive_and_support_regex() {
        let mut file = draft();
        file.rules = vec![
            Rule {
                matcher: Matcher {
                    process: Some("code.exe".into()),
                    title: Some("^main\\.rs".into()),
                    class: None,
                },
                context: "vscode".into(),
            },
            Rule {
                matcher: Matcher {
                    process: Some("code.exe".into()),
                    title: None,
                    class: None,
                },
                context: "general".into(),
            },
        ];
        let runtime = compile(file, PathBuf::from("test.yaml")).expect("compile");

        assert_eq!(
            runtime
                .resolve_context(&focus("Code.exe", "main.rs - Visual Studio Code"))
                .as_str(),
            "vscode"
        );
        assert_eq!(
            runtime
                .resolve_context(&focus("code.exe", "settings.json"))
                .as_str(),
            "general"
        );
        assert_eq!(
            runtime
                .resolve_context(&focus("vlc.exe", "anything"))
                .as_str(),
            runtime.default_context
        );
    }

    #[test]
    fn validation_rejects_empty_rules_and_unknown_layers() {
        let mut file = draft();
        file.rules = vec![Rule {
            matcher: Matcher::default(),
            context: "general".into(),
        }];
        assert!(compile(file, PathBuf::from("test.yaml")).is_err());

        let mut file = draft();
        file.contexts
            .get_mut("general")
            .expect("general context")
            .layers
            .push("missing-layer".into());
        assert!(compile(file, PathBuf::from("test.yaml")).is_err());

        let mut file = draft();
        file.contexts.clear();
        file.rules.clear();
        assert!(compile(file, PathBuf::from("test.yaml")).is_err());
    }
}
