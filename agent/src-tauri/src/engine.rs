use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use crate::actions;
use crate::config::{Action, Input, LayerMode, Runtime};
use crate::context::{self, FocusInfo};
use crate::input::{self, InputEvent};
use crate::osd;

#[derive(Debug, Clone, Copy)]
pub enum Command {
    Reload,
    Legend,
}

static CMD_TX: OnceLock<Mutex<mpsc::Sender<Command>>> = OnceLock::new();

pub fn request_legend() {
    if let Some(tx) = CMD_TX.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(Command::Legend);
        }
    }
}

struct GestureState {
    down_at: Instant,
    hold_fired: bool,
    second_tap: bool,
    layer: String,
}

struct Engine {
    runtime: Runtime,
    app: AppHandle,
    own_exe: String,
    last_focus: Option<FocusInfo>,
    context: String,
    manual_layer: Option<String>,
    overlay: Option<(String, Input)>,
    oneshot: Option<String>,
    peek_input: Option<Input>,
    pressed: HashSet<Input>,
    states: HashMap<Input, GestureState>,
    pending_taps: HashMap<Input, (Instant, String)>,
    last_knob: Option<Instant>,
    last_tooltip: String,
    last_tray_icon: String,
}

impl Engine {
    fn new(runtime: Runtime, app: AppHandle) -> Self {
        let context = runtime.default_context.clone();
        let own_exe = std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase())
            })
            .unwrap_or_default();
        Self {
            runtime,
            app,
            own_exe,
            last_focus: None,
            context,
            manual_layer: None,
            overlay: None,
            oneshot: None,
            peek_input: None,
            pressed: HashSet::new(),
            states: HashMap::new(),
            pending_taps: HashMap::new(),
            last_knob: None,
            last_tooltip: String::new(),
            last_tray_icon: String::new(),
        }
    }

    fn log(&self, message: impl AsRef<str>) {
        crate::log_info!("[engine] {}", message.as_ref());
    }

    /// Keep the taskbar/tray status in sync: layer colour as the icon,
    /// context · layer (plus the last action) as the tooltip.
    fn update_tray(&mut self, action: Option<&str>) {
        let context = self.runtime.context_label(&self.context);
        let layer_name = self.current_layer();
        let layer_label = self
            .runtime
            .layer(&layer_name)
            .map(|layer| layer.title(&layer_name))
            .unwrap_or_else(|| layer_name.clone());
        let color = self
            .runtime
            .layer(&layer_name)
            .map(|layer| layer.color())
            .unwrap_or_else(|| "#7dd3fc".into());

        let mut tooltip = format!("macrokey · {context} · {layer_label}");
        if let Some(action) = action {
            if !action.is_empty() {
                tooltip.push_str(" · ");
                tooltip.push_str(action);
            }
        }
        let tooltip = crate::tray::short(&tooltip, 120);

        let Some(tray) = self.app.tray_by_id("macrokey") else {
            return;
        };
        if tooltip != self.last_tooltip {
            let _ = tray.set_tooltip(Some(tooltip.clone()));
            crate::log_info!("[tray] {tooltip}  ({color})");
            self.last_tooltip = tooltip;
        }
        if color != self.last_tray_icon {
            let _ = tray.set_icon(Some(crate::tray::icon(&color)));
            self.last_tray_icon = color;
        }
    }

    fn current_layer(&self) -> String {
        if let Some((layer, _)) = &self.overlay {
            return layer.clone();
        }
        if let Some(layer) = &self.oneshot {
            return layer.clone();
        }
        if let Some(layer) = &self.manual_layer {
            if self
                .runtime
                .context(&self.context)
                .map(|context| context.contains(layer))
                .unwrap_or(false)
            {
                return layer.clone();
            }
        }
        self.runtime.context_default_layer(&self.context)
    }

    fn show_layer(
        &self,
        badge: Option<&str>,
        sticky: bool,
        pressed: Option<Input>,
        action: Option<&str>,
    ) {
        if !self.runtime.settings.osd.enabled {
            return;
        }
        let layer = self.current_layer();
        let mode = if pressed.is_some() {
            "press"
        } else if sticky {
            "peek"
        } else {
            "layer"
        };
        let payload = osd::layer_payload(
            &self.runtime,
            &self.context,
            &layer,
            badge,
            pressed.map(|input| input.key()),
            action,
            mode,
        );
        let duration = self.runtime.settings.osd.duration_ms;
        osd::show(
            &self.app,
            payload,
            duration,
            sticky,
            &self.runtime.settings.osd.monitor,
        );
    }

    fn on_focus(&mut self, focus: FocusInfo) {
        if focus.exe.eq_ignore_ascii_case(&self.own_exe) {
            return;
        }
        self.last_focus = Some(focus.clone());
        let resolved = self.runtime.resolve_context(&focus);
        if resolved == self.context {
            return;
        }

        self.context = resolved.clone();
        self.manual_layer = None;
        self.overlay = None;
        self.oneshot = None;
        self.pending_taps.clear();
        self.update_tray(None);

        self.log(format!(
            "context {} | {} -> {}",
            focus.exe,
            shorten(&focus.title, 60),
            resolved
        ));
        if self.runtime.settings.osd.show_on_focus {
            self.show_layer(None, false, None, None);
        } else {
            // Never leave the last press's HUD floating over the new app.
            osd::hide(&self.app);
        }
    }

    fn on_input(&mut self, event: InputEvent) {
        if event.down {
            self.on_down(event);
        } else {
            self.on_up(event);
        }
    }

    fn on_down(&mut self, event: InputEvent) {
        let input = event.input;
        if !self.pressed.insert(input) {
            return; // Windows auto-repeat while held
        }

        // Resolve a pending first tap before the new press. If its double-tap
        // window closed between ticks, run the single action now instead of
        // losing it; the new press may then resolve against a new layer.
        let mut second_tap = false;
        if let Some((deadline, pending_layer)) = self.pending_taps.remove(&input) {
            if event.at <= deadline {
                second_tap = true;
            } else if let Some(action) = self
                .runtime
                .binding(&pending_layer, input)
                .and_then(|b| b.tap())
                .cloned()
            {
                self.execute_action(&action, input, pending_layer, 1);
            }
        }

        let layer = self.current_layer();
        let binding = self.runtime.binding(&layer, input).cloned();

        self.states.insert(
            input,
            GestureState {
                down_at: event.at,
                hold_fired: false,
                second_tap,
                layer: layer.clone(),
            },
        );

        if input.is_rotation() {
            let multiplier = self.knob_multiplier(event.at);
            if let Some(action) = binding.as_ref().and_then(|b| b.tap()).cloned() {
                self.execute_action(&action, input, layer, multiplier);
            }
            self.states.remove(&input);
        }
    }

    fn on_up(&mut self, event: InputEvent) {
        let input = event.input;
        self.pressed.remove(&input);
        let Some(state) = self.states.remove(&input) else {
            return;
        };
        if input.is_rotation() {
            return;
        }

        let layer = state.layer.clone();
        let binding = self.runtime.binding(&layer, input).cloned();
        let held = event.at.duration_since(state.down_at);

        if state.hold_fired {
            self.release_hold(input);
            return;
        }

        if let Some(hold_action) = binding.as_ref().and_then(|b| b.hold()).cloned() {
            if held.as_millis() >= self.hold_ms() as u128 {
                self.fire_hold(hold_action, input, layer);
                self.release_hold(input);
                return;
            }
        }

        if state.second_tap {
            if let Some(action) = binding.as_ref().and_then(|b| b.double()).cloned() {
                self.execute_action(&action, input, layer, 1);
            } else if let Some(action) = binding.as_ref().and_then(|b| b.tap()).cloned() {
                self.execute_action(&action, input, layer, 1);
            }
            return;
        }

        // A double binding can exist without a tap binding. In that case the
        // first press still has to open the double-tap window, even though its
        // expiry runs no action.
        if binding.as_ref().and_then(|b| b.double()).is_some() {
            let deadline = event.at + Duration::from_millis(self.double_tap_ms());
            self.pending_taps.insert(input, (deadline, layer));
        } else if let Some(action) = binding.as_ref().and_then(|b| b.tap()).cloned() {
            self.execute_action(&action, input, layer, 1);
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();

        let holds: Vec<(Input, String)> = self
            .states
            .iter()
            .filter_map(|(input, state)| {
                if state.hold_fired || input.is_rotation() {
                    return None;
                }
                let binding = self.runtime.binding(&state.layer, *input)?;
                binding.hold()?;
                if now.duration_since(state.down_at).as_millis() >= self.hold_ms() as u128 {
                    Some((*input, state.layer.clone()))
                } else {
                    None
                }
            })
            .collect();

        for (input, layer) in holds {
            if let Some(state) = self.states.get_mut(&input) {
                state.hold_fired = true;
            }
            if let Some(action) = self
                .runtime
                .binding(&layer, input)
                .and_then(|b| b.hold())
                .cloned()
            {
                self.fire_hold(action, input, layer);
            }
        }

        let expired: Vec<Input> = self
            .pending_taps
            .iter()
            .filter(|(_, (deadline, _))| now >= *deadline)
            .map(|(input, _)| *input)
            .collect();
        for input in expired {
            if let Some((_, layer)) = self.pending_taps.remove(&input) {
                if let Some(action) = self
                    .runtime
                    .binding(&layer, input)
                    .and_then(|b| b.tap())
                    .cloned()
                {
                    self.execute_action(&action, input, layer, 1);
                }
            }
        }
    }

    fn double_tap_ms(&self) -> u64 {
        self.runtime.settings.gestures.double_tap_ms
    }

    fn hold_ms(&self) -> u64 {
        self.runtime.settings.gestures.hold_ms
    }

    fn knob_multiplier(&mut self, now: Instant) -> u32 {
        let settings = &self.runtime.settings.knob;
        let multiplier = if !settings.acceleration {
            1
        } else if let Some(previous) = self.last_knob {
            let elapsed = now.duration_since(previous).as_millis() as u64;
            let window = settings.window_ms.max(1);
            let table = &settings.multiplier;
            let m = if elapsed < window / 4 {
                *table.get(4).unwrap_or(&8)
            } else if elapsed < window / 2 {
                *table.get(3).unwrap_or(&4)
            } else if elapsed < window {
                *table.get(2).unwrap_or(&2)
            } else {
                *table.first().unwrap_or(&1)
            };
            m.max(1)
        } else {
            1
        };
        self.last_knob = Some(now);
        multiplier
    }

    fn fire_hold(&mut self, action: Action, input: Input, layer: String) {
        if matches!(action, Action::Layer { .. }) && self.oneshot.take().is_some() {
            // A one-shot layer would otherwise hide the layer transition that
            // this hold is about to perform.
            self.update_tray(None);
        }
        match &action {
            Action::Layer { mode, name } => match mode {
                LayerMode::Momentary => {
                    if let Some(name) = name {
                        self.overlay = Some((name.clone(), input));
                        self.update_tray(None);
                        self.show_layer(Some("held"), true, Some(input), None);
                    }
                }
                LayerMode::Switch => {
                    if let Some(name) = name {
                        self.switch_layer(name.clone(), input);
                    }
                }
                LayerMode::Oneshot => {
                    if let Some(name) = name {
                        self.oneshot = Some(name.clone());
                        self.update_tray(None);
                        self.show_layer(Some("once"), false, Some(input), None);
                    }
                }
                LayerMode::Next => self.next_layer(),
                LayerMode::Prev => self.prev_layer(),
                LayerMode::Back => {
                    self.manual_layer = None;
                    self.update_tray(None);
                    self.show_layer(Some("base"), false, Some(input), None);
                }
            },
            Action::Peek => {
                self.peek_input = Some(input);
                self.show_layer(Some("held"), true, None, None);
            }
            _ => self.execute_action(&action, input, layer, 1),
        }
    }

    fn release_hold(&mut self, input: Input) {
        if self.peek_input == Some(input) {
            self.peek_input = None;
            osd::hide(&self.app);
            return;
        }
        if self.overlay.as_ref().map(|(_, held)| *held) == Some(input) {
            self.overlay = None;
            self.update_tray(None);
            self.show_layer(None, false, None, None);
        }
    }

    fn execute_action(&mut self, action: &Action, input: Input, layer: String, multiplier: u32) {
        // Any executed action consumes a one-shot layer. A layer action may then
        // install a new one. Reflect the base layer immediately, including for
        // Peek/Nothing/failed actions that do not update the tray later.
        if self.oneshot.take().is_some() {
            self.update_tray(None);
        }

        match action {
            Action::Layer { mode, name } => {
                match mode {
                    LayerMode::Switch => {
                        if let Some(name) = name {
                            self.switch_layer(name.clone(), input);
                        }
                    }
                    // Momentary cannot be held from a tap; degrade to one-shot.
                    LayerMode::Momentary | LayerMode::Oneshot => {
                        if let Some(name) = name {
                            self.oneshot = Some(name.clone());
                            self.update_tray(None);
                            self.show_layer(Some("once"), false, Some(input), None);
                        }
                    }
                    LayerMode::Next => self.next_layer(),
                    LayerMode::Prev => self.prev_layer(),
                    LayerMode::Back => {
                        self.manual_layer = None;
                        self.update_tray(None);
                        self.show_layer(Some("base"), false, Some(input), None);
                    }
                }
                return;
            }
            Action::Peek => {
                self.show_layer(None, false, Some(input), None);
                return;
            }
            Action::Nothing => return,
            _ => {}
        }

        self.log(format!(
            "{input:?} -> {} [{} / {}]{}",
            action.label(),
            self.runtime.context_label(&self.context),
            self.runtime
                .layer(&layer)
                .map(|l| l.title(&layer))
                .unwrap_or_else(|| layer.clone()),
            if multiplier > 1 {
                format!(" x{multiplier}")
            } else {
                String::new()
            }
        ));

        let result = match action {
            Action::Wheel { delta, modifiers } => {
                actions::send_wheel(*delta * multiplier as i32, modifiers)
            }
            Action::Hotkey { keys, repeat } => {
                let times = if *repeat { multiplier } else { 1 };
                let mut result = Ok(());
                for _ in 0..times {
                    result = actions::send_hotkey(keys);
                    if result.is_err() {
                        break;
                    }
                }
                result
            }
            other => actions::execute(other),
        };

        if let Err(error) = result {
            crate::log_error!("[engine] action failed: {error:#}");
        } else {
            let label = action.label();
            self.update_tray(Some(&label));
            if self.runtime.settings.osd.show_on_press {
                self.show_layer(Some("pressed"), false, Some(input), Some(&label));
            }
        }
    }

    fn switch_layer(&mut self, layer: String, input: Input) {
        let allowed = self
            .runtime
            .context(&self.context)
            .map(|context| context.contains(&layer))
            .unwrap_or(false);
        if !allowed {
            self.log(format!(
                "layer {layer:?} is not part of context {:?}",
                self.context
            ));
            return;
        }
        self.log(format!("layer -> {layer} (context {})", self.context));
        self.manual_layer = Some(layer);
        self.update_tray(None);
        self.show_layer(Some("manual"), false, Some(input), None);
    }

    fn next_layer(&mut self) {
        self.cycle_layer(1);
    }

    fn prev_layer(&mut self) {
        self.cycle_layer(-1);
    }

    fn cycle_layer(&mut self, step: i32) {
        let layers = self.runtime.context_layers(&self.context);
        if layers.is_empty() {
            return;
        }
        let current = self.current_layer();
        let index = layers
            .iter()
            .position(|layer| *layer == current)
            .unwrap_or(0);
        let count = layers.len() as i32;
        let next = layers[((index as i32 + step).rem_euclid(count)) as usize].clone();
        self.log(format!("layer -> {next} (context {})", self.context));
        self.manual_layer = Some(next);
        self.update_tray(None);
        self.show_layer(Some("manual"), false, None, None);
    }

    fn reload(&mut self) {
        match crate::config::load_runtime() {
            Ok(runtime) => {
                self.runtime = runtime;
                self.context = self
                    .last_focus
                    .as_ref()
                    .map(|focus| self.runtime.resolve_context(focus))
                    .unwrap_or_else(|| self.runtime.default_context.clone());
                self.manual_layer = None;
                self.overlay = None;
                self.oneshot = None;
                self.peek_input = None;
                self.pressed.clear();
                self.states.clear();
                self.pending_taps.clear();
                self.update_tray(None);
                self.log("config reloaded");
            }
            Err(error) => crate::log_error!("[engine] reload failed: {error:#}"),
        }
    }
}

fn shorten(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

pub fn spawn(app: AppHandle, runtime: Runtime) {
    let (input_tx, input_rx) = mpsc::channel();
    let (focus_tx, focus_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let watch_tx = cmd_tx.clone();
    let db_path = runtime.path.clone();
    let _ = CMD_TX.set(Mutex::new(cmd_tx));

    std::thread::spawn(move || {
        let mut last = crate::db::revision(&db_path).unwrap_or(0);
        loop {
            std::thread::sleep(Duration::from_millis(600));
            if let Ok(revision) = crate::db::revision(&db_path) {
                if revision != last {
                    last = revision;
                    let _ = watch_tx.send(Command::Reload);
                }
            }
        }
    });

    input::spawn(input_tx, runtime.inputs.clone(), runtime.vid, runtime.pid);
    context::spawn(focus_tx);

    std::thread::spawn(move || {
        let mut engine = Engine::new(runtime, app);
        engine.update_tray(None);
        engine.log(format!(
            "started; config {}; context {}",
            engine.runtime.path.display(),
            engine.context
        ));
        loop {
            while let Ok(command) = cmd_rx.try_recv() {
                match command {
                    Command::Reload => engine.reload(),
                    Command::Legend => engine.show_layer(None, false, None, None),
                }
            }
            while let Ok(focus) = focus_rx.try_recv() {
                engine.on_focus(focus);
            }
            match input_rx.recv_timeout(Duration::from_millis(35)) {
                Ok(event) => engine.on_input(event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            while let Ok(event) = input_rx.try_recv() {
                engine.on_input(event);
            }
            engine.tick();
        }
    });
}
