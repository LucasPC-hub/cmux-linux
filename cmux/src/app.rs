//! Application entry point — creates the AdwApplication and main window.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use gtk4::prelude::*;
use libadwaita as adw;
use tokio::sync::mpsc::UnboundedSender;
use vte4::prelude::*;

/// Lock a mutex, recovering from poisoning rather than panicking.
/// Prevents cascading panics when one thread panics while holding a lock.
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::error!("Mutex was poisoned, recovering");
        poisoned.into_inner()
    })
}

use crate::model::TabManager;
use crate::notifications::NotificationStore;
use crate::socket;
use crate::ui;
use uuid::Uuid;

/// Shared application state accessible from UI callbacks (single-threaded, GTK main thread).
pub struct AppState {
    pub shared: Arc<SharedState>,
    pub terminal_cache: RefCell<HashMap<Uuid, vte4::Terminal>>,
}

impl AppState {
    pub fn new(shared: Arc<SharedState>) -> Self {
        Self {
            shared,
            terminal_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn terminal_for(
        &self,
        panel_id: Uuid,
        working_directory: Option<&str>,
    ) -> vte4::Terminal {
        if let Some(terminal) = self.terminal_cache.borrow().get(&panel_id) {
            return terminal.clone();
        }

        let terminal = vte4::Terminal::new();
        terminal.set_hexpand(true);
        terminal.set_vexpand(true);
        terminal.set_scrollback_lines(10000);

        // Track terminal title changes → update workspace process_title
        {
            let shared = self.shared.clone();
            let pid = panel_id;
            terminal.connect_window_title_changed(move |term| {
                if let Some(title) = term.window_title() {
                    let title = title.to_string();
                    if !title.is_empty() {
                        let mut tab_manager = lock_or_recover(&shared.tab_manager);
                        if let Some(ws) = tab_manager.find_workspace_with_panel_mut(pid) {
                            ws.process_title = title;
                        }
                        drop(tab_manager);
                        shared.notify_ui_refresh();
                    }
                }
            });
        }

        // Spawn shell
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let shell_args: Vec<&str> = vec![&shell];
        let envv: Vec<&str> = vec![];

        terminal.spawn_async(
            vte4::PtyFlags::DEFAULT,
            working_directory,
            &shell_args,
            &envv,
            glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            None::<&gio::Cancellable>,
            |result| {
                match result {
                    Ok(_pid) => tracing::debug!("Shell spawned successfully"),
                    Err(e) => tracing::error!("Failed to spawn shell: {}", e),
                }
            },
        );

        self.terminal_cache
            .borrow_mut()
            .insert(panel_id, terminal.clone());
        terminal
    }

    pub fn send_input_to_panel(&self, panel_id: Uuid, text: &str) -> bool {
        let terminal = if let Some(terminal) = self.terminal_cache.borrow().get(&panel_id).cloned()
        {
            terminal
        } else {
            let working_directory = {
                let tab_manager = lock_or_recover(&self.shared.tab_manager);
                let Some(workspace) = tab_manager.find_workspace_with_panel(panel_id) else {
                    return false;
                };
                let Some(panel) = workspace.panel(panel_id) else {
                    return false;
                };
                if panel.panel_type != crate::model::PanelType::Terminal {
                    return false;
                }
                panel.directory.clone()
            };
            self.terminal_for(panel_id, working_directory.as_deref())
        };

        terminal.feed_child(text.as_bytes());
        true
    }

    pub fn close_panel(&self, panel_id: Uuid, process_alive: bool) -> bool {
        {
            let mut tab_manager = lock_or_recover(&self.shared.tab_manager);
            let Some(workspace) = tab_manager.find_workspace_with_panel_mut(panel_id) else {
                return false;
            };
            if !workspace.remove_panel(panel_id) {
                return false;
            }
            let empty_workspace_id = workspace.is_empty().then_some(workspace.id);
            if let Some(workspace_id) = empty_workspace_id {
                tab_manager.remove_by_id(workspace_id);
            }
        }

        self.terminal_cache.borrow_mut().remove(&panel_id);
        self.shared.notify_ui_refresh();
        tracing::debug!(%panel_id, process_alive, "closed terminal panel");
        true
    }

    pub fn prune_terminal_cache(&self) {
        let live_panels: HashSet<Uuid> = {
            let tab_manager = lock_or_recover(&self.shared.tab_manager);
            tab_manager
                .iter()
                .flat_map(|workspace| workspace.panels.values())
                .filter(|panel| panel.panel_type == crate::model::PanelType::Terminal)
                .map(|panel| panel.id)
                .collect()
        };

        self.terminal_cache
            .borrow_mut()
            .retain(|panel_id, _| live_panels.contains(panel_id));
    }
}

/// Messages from background tasks that require a UI refresh.
#[derive(Clone, Debug)]
pub enum UiEvent {
    Refresh,
    SendInput { panel_id: Uuid, text: String },
}

/// Thread-safe state shared between GTK main thread and socket server.
/// The socket server reads/writes through this, then signals the GTK main thread
/// via glib channels for UI updates.
pub struct SharedState {
    pub tab_manager: Mutex<TabManager>,
    pub notifications: Mutex<NotificationStore>,
    ui_event_tx: Mutex<Option<UnboundedSender<UiEvent>>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            tab_manager: Mutex::new(TabManager::new()),
            notifications: Mutex::new(NotificationStore::new()),
            ui_event_tx: Mutex::new(None),
        }
    }

    pub fn install_ui_event_sender(&self, sender: UnboundedSender<UiEvent>) {
        *lock_or_recover(&self.ui_event_tx) = Some(sender);
    }

    pub fn send_ui_event(&self, event: UiEvent) -> bool {
        lock_or_recover(&self.ui_event_tx)
            .as_ref()
            .is_some_and(|sender| sender.send(event).is_ok())
    }

    pub fn notify_ui_refresh(&self) {
        let _ = self.send_ui_event(UiEvent::Refresh);
    }
}

/// Run the GTK application. Returns the exit code.
pub fn run() -> i32 {
    let app = adw::Application::builder()
        .application_id("ai.manaflow.cmux")
        .build();

    let shared = Arc::new(SharedState::new());
    let state = Rc::new(AppState::new(shared.clone()));

    {
        let shared_for_socket = shared.clone();
        app.connect_startup(move |_app| {
            let shared = shared_for_socket.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
                rt.block_on(async {
                    if let Err(e) = socket::server::run_socket_server(shared).await {
                        tracing::error!("Socket server error: {}", e);
                    }
                });
            });
        });
    }

    let state_clone = state.clone();
    app.connect_activate(move |app| {
        activate(app, &state_clone);
    });

    app.connect_shutdown(|_app| {
        socket::server::cleanup();
        tracing::info!("Application shutdown");
    });

    app.run().into()
}

fn activate(app: &adw::Application, state: &Rc<AppState>) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    let (ui_event_tx, ui_event_rx) = tokio::sync::mpsc::unbounded_channel();
    state.shared.install_ui_event_sender(ui_event_tx);

    // Create the main window
    let window = ui::window::create_window(app, state, ui_event_rx);
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_panel_removes_last_workspace() {
        let shared = Arc::new(SharedState::new());
        let state = AppState::new(shared.clone());
        let panel_id = shared
            .tab_manager
            .lock()
            .unwrap()
            .selected()
            .and_then(|workspace| workspace.focused_panel_id)
            .expect("workspace should have a focused panel");

        assert!(state.close_panel(panel_id, false));
        assert!(shared.tab_manager.lock().unwrap().is_empty());
    }

    #[test]
    fn close_panel_returns_false_for_unknown_panel() {
        let state = AppState::new(Arc::new(SharedState::new()));
        assert!(!state.close_panel(Uuid::new_v4(), true));
    }
}
