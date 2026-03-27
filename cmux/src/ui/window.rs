//! Main application window using AdwNavigationSplitView.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use tokio::sync::mpsc::UnboundedReceiver;
use vte4::prelude::*;
use webkit6::prelude::*;

use crate::app::{lock_or_recover, AppState, UiEvent};
use crate::model::panel::SplitOrientation;
use crate::model::{PanelType, Workspace};
use crate::ui::{sidebar, split_view};

/// Create the main application window.
pub fn create_window(
    app: &adw::Application,
    state: &Rc<AppState>,
    ui_events: UnboundedReceiver<UiEvent>,
) -> adw::ApplicationWindow {
    install_css();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("cmux")
        .default_width(1280)
        .default_height(860)
        .build();

    let split_view = adw::NavigationSplitView::new();
    split_view.set_min_sidebar_width(220.0);
    split_view.set_max_sidebar_width(360.0);
    split_view.set_vexpand(true);
    split_view.set_hexpand(true);

    let sidebar_widgets = sidebar::create_sidebar(state);
    let list_box = sidebar_widgets.list_box.clone();
    let sidebar_page = adw::NavigationPage::new(&sidebar_widgets.root, "Workspaces");
    split_view.set_sidebar(Some(&sidebar_page));

    let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content_box.set_hexpand(true);
    content_box.set_vexpand(true);
    rebuild_content(&content_box, state);

    let content_page = adw::NavigationPage::new(&content_box, "Terminal");
    split_view.set_content(Some(&content_page));

    bind_sidebar_selection(&list_box, &content_box, state);
    bind_shared_state_updates(&list_box, &content_box, state, ui_events);

    // --- Actions ---
    install_actions(&window, app, state, &list_box, &content_box);

    // --- Header bar ---
    let header = adw::HeaderBar::new();

    let new_ws_btn = gtk4::Button::from_icon_name("tab-new-symbolic");
    new_ws_btn.set_tooltip_text(Some("New Workspace (Ctrl+Shift+T)"));
    new_ws_btn.set_action_name(Some("win.new-workspace"));
    header.pack_start(&new_ws_btn);

    let split_h_btn = gtk4::Button::from_icon_name("view-dual-symbolic");
    split_h_btn.set_tooltip_text(Some("Split Right (Ctrl+Shift+D)"));
    split_h_btn.set_action_name(Some("win.split-right"));
    header.pack_start(&split_h_btn);

    let split_v_btn = gtk4::Button::from_icon_name("view-paged-symbolic");
    split_v_btn.set_tooltip_text(Some("Split Down (Ctrl+Shift+E)"));
    split_v_btn.set_action_name(Some("win.split-down"));
    header.pack_start(&split_v_btn);

    // Hamburger menu
    let menu = build_app_menu();
    let menu_btn = gtk4::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    menu_btn.set_menu_model(Some(&menu));
    menu_btn.set_tooltip_text(Some("Menu"));
    header.pack_end(&menu_btn);

    let outer_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    outer_box.append(&header);
    outer_box.append(&split_view);

    window.set_content(Some(&outer_box));

    // VTE terminals handle focus natively via GTK

    window
}

/// Build the hamburger menu model.
fn build_app_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    // Workspaces section
    let ws_section = gio::Menu::new();
    ws_section.append(Some("New Workspace"), Some("win.new-workspace"));
    ws_section.append(Some("Close Workspace"), Some("win.close-workspace"));
    ws_section.append(Some("Rename Workspace"), Some("win.rename-workspace"));
    menu.append_section(Some("Workspaces"), &ws_section);

    // Panes section
    let pane_section = gio::Menu::new();
    pane_section.append(Some("Split Right"), Some("win.split-right"));
    pane_section.append(Some("Split Down"), Some("win.split-down"));
    pane_section.append(Some("Split Browser Right"), Some("win.split-browser-right"));
    pane_section.append(Some("Close Pane"), Some("win.close-pane"));
    menu.append_section(Some("Panes"), &pane_section);

    // Terminal section
    let term_section = gio::Menu::new();
    term_section.append(Some("Copy"), Some("win.copy"));
    term_section.append(Some("Paste"), Some("win.paste"));
    term_section.append(Some("Increase Font Size"), Some("win.font-increase"));
    term_section.append(Some("Decrease Font Size"), Some("win.font-decrease"));
    term_section.append(Some("Reset Font Size"), Some("win.font-reset"));
    menu.append_section(Some("Terminal"), &term_section);

    // Notifications section
    let notif_section = gio::Menu::new();
    notif_section.append(Some("Jump to Latest Unread"), Some("win.jump-unread"));
    menu.append_section(Some("Notifications"), &notif_section);

    // App section
    let app_section = gio::Menu::new();
    app_section.append(Some("About cmux"), Some("win.about"));
    app_section.append(Some("Quit"), Some("app.quit"));
    menu.append_section(None, &app_section);

    menu
}

/// Install all window actions with keyboard accelerators.
fn install_actions(
    window: &adw::ApplicationWindow,
    app: &adw::Application,
    state: &Rc<AppState>,
    list_box: &gtk4::ListBox,
    content_box: &gtk4::Box,
) {
    use gio::SimpleAction;

    // Helper macro to reduce boilerplate
    macro_rules! add_action {
        ($name:expr, $accel:expr, $body:expr) => {{
            let action = SimpleAction::new($name, None);
            let s = state.clone();
            let lb = list_box.clone();
            let cb = content_box.clone();
            action.connect_activate(move |_, _| {
                ($body)(&s, &lb, &cb);
            });
            window.add_action(&action);
            if !$accel.is_empty() {
                app.set_accels_for_action(&format!("win.{}", $name), &[$accel]);
            }
        }};
    }

    // --- Workspace actions ---
    add_action!("new-workspace", "<Ctrl><Shift>t", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        let workspace = Workspace::new();
        lock_or_recover(&state.shared.tab_manager).add_workspace(workspace);
        refresh_ui(lb, cb, state);
    });

    add_action!("close-workspace", "<Ctrl><Shift>w", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        let mut tab_manager = lock_or_recover(&state.shared.tab_manager);
        if let Some(index) = tab_manager.selected_index() {
            tab_manager.remove(index);
        }
        drop(tab_manager);
        refresh_ui(lb, cb, state);
    });

    add_action!("close-pane", "<Ctrl>w", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        let panel_id = {
            let tab_manager = lock_or_recover(&state.shared.tab_manager);
            tab_manager.selected().and_then(|ws| ws.focused_panel_id)
        };
        if let Some(panel_id) = panel_id {
            state.close_panel(panel_id, true);
            refresh_ui(lb, cb, state);
        }
    });

    // Rename workspace (opens a dialog)
    {
        let action = SimpleAction::new("rename-workspace", None);
        let s = state.clone();
        let lb = list_box.clone();
        let cb = content_box.clone();
        let w = window.clone();
        action.connect_activate(move |_, _| {
            show_rename_dialog(&w, &s, &lb, &cb);
        });
        window.add_action(&action);
        app.set_accels_for_action("win.rename-workspace", &["<Ctrl><Shift>r"]);
    }

    // --- Split actions ---
    add_action!("split-right", "<Ctrl><Shift>d", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        if let Some(workspace) = lock_or_recover(&state.shared.tab_manager).selected_mut() {
            workspace.split(SplitOrientation::Horizontal, PanelType::Terminal);
        }
        refresh_ui(lb, cb, state);
    });

    add_action!("split-down", "<Ctrl><Shift>e", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        if let Some(workspace) = lock_or_recover(&state.shared.tab_manager).selected_mut() {
            workspace.split(SplitOrientation::Vertical, PanelType::Terminal);
        }
        refresh_ui(lb, cb, state);
    });

    add_action!("split-browser-right", "<Ctrl><Shift>b", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        if let Some(workspace) = lock_or_recover(&state.shared.tab_manager).selected_mut() {
            workspace.split(SplitOrientation::Horizontal, PanelType::Browser);
        }
        refresh_ui(lb, cb, state);
    });

    add_action!("split-browser-down", "<Ctrl><Shift>n", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        if let Some(workspace) = lock_or_recover(&state.shared.tab_manager).selected_mut() {
            workspace.split(SplitOrientation::Vertical, PanelType::Browser);
        }
        refresh_ui(lb, cb, state);
    });

    // --- Terminal actions (operate on focused VTE terminal) ---
    {
        let action = SimpleAction::new("copy", None);
        let s = state.clone();
        action.connect_activate(move |_, _| {
            if let Some(term) = get_focused_terminal(&s) {
                term.copy_clipboard_format(vte4::Format::Text);
            }
        });
        window.add_action(&action);
        app.set_accels_for_action("win.copy", &["<Ctrl><Shift>c"]);
    }

    {
        let action = SimpleAction::new("paste", None);
        let s = state.clone();
        action.connect_activate(move |_, _| {
            if let Some(term) = get_focused_terminal(&s) {
                term.paste_clipboard();
            }
        });
        window.add_action(&action);
        app.set_accels_for_action("win.paste", &["<Ctrl><Shift>v"]);
    }

    {
        let action = SimpleAction::new("font-increase", None);
        let s = state.clone();
        action.connect_activate(move |_, _| {
            change_font_scale(&s, 0.1);
        });
        window.add_action(&action);
        app.set_accels_for_action("win.font-increase", &["<Ctrl>plus", "<Ctrl>equal"]);
    }

    {
        let action = SimpleAction::new("font-decrease", None);
        let s = state.clone();
        action.connect_activate(move |_, _| {
            change_font_scale(&s, -0.1);
        });
        window.add_action(&action);
        app.set_accels_for_action("win.font-decrease", &["<Ctrl>minus"]);
    }

    {
        let action = SimpleAction::new("font-reset", None);
        let s = state.clone();
        action.connect_activate(move |_, _| {
            for terminal in s.terminal_cache.borrow().values() {
                terminal.set_font_scale(1.0);
            }
        });
        window.add_action(&action);
        app.set_accels_for_action("win.font-reset", &["<Ctrl>0"]);
    }

    // --- Notifications ---
    add_action!("jump-unread", "<Ctrl><Shift>u", |state: &Rc<AppState>, lb: &gtk4::ListBox, cb: &gtk4::Box| {
        if select_latest_unread(state) {
            refresh_ui(lb, cb, state);
        }
    });

    // --- Workspace jump (Ctrl+1-9) ---
    for i in 1..=9u32 {
        let action_name = format!("jump-workspace-{}", i);
        let action = SimpleAction::new(&action_name, None);
        let s = state.clone();
        let lb = list_box.clone();
        let cb = content_box.clone();
        let index = if i == 9 { usize::MAX } else { (i - 1) as usize };
        action.connect_activate(move |_, _| {
            let target = if index == usize::MAX {
                // Jump to last workspace
                let tab_manager = lock_or_recover(&s.shared.tab_manager);
                let len = tab_manager.len();
                if len > 0 { len - 1 } else { return; }
            } else {
                index
            };
            if select_workspace_by_index(&s, target) {
                refresh_ui(&lb, &cb, &s);
            }
        });
        window.add_action(&action);
        app.set_accels_for_action(
            &format!("win.jump-workspace-{}", i),
            &[&format!("<Alt>{}", i)],
        );
    }

    // --- About ---
    {
        let action = SimpleAction::new("about", None);
        let w = window.clone();
        action.connect_activate(move |_, _| {
            let about = adw::AboutDialog::builder()
                .application_name("cmux")
                .version("0.1.0")
                .developer_name("LucasPC-hub")
                .license_type(gtk4::License::Agpl30)
                .website("https://github.com/LucasPC-hub/cmux-linux")
                .issue_url("https://github.com/LucasPC-hub/cmux-linux/issues")
                .comments("Terminal multiplexer for AI coding agents.\nLinux/VTE fork of cmux by Manaflow.")
                .build();
            about.present(Some(&w));
        });
        window.add_action(&action);
    }

    // --- App quit ---
    {
        let action = SimpleAction::new("quit", None);
        let a = app.clone();
        action.connect_activate(move |_, _| {
            a.quit();
        });
        app.add_action(&action);
        app.set_accels_for_action("app.quit", &["<Ctrl>q"]);
    }
}

fn get_focused_terminal(state: &Rc<AppState>) -> Option<vte4::Terminal> {
    let panel_id = {
        let tab_manager = lock_or_recover(&state.shared.tab_manager);
        tab_manager.selected().and_then(|ws| ws.focused_panel_id)
    }?;
    state.terminal_cache.borrow().get(&panel_id).cloned()
}

fn change_font_scale(state: &Rc<AppState>, delta: f64) {
    for terminal in state.terminal_cache.borrow().values() {
        let current = terminal.font_scale();
        let new_scale = (current + delta).clamp(0.5, 3.0);
        terminal.set_font_scale(new_scale);
    }
}

fn show_rename_dialog(
    window: &adw::ApplicationWindow,
    state: &Rc<AppState>,
    list_box: &gtk4::ListBox,
    content_box: &gtk4::Box,
) {
    let current_title = {
        let tab_manager = lock_or_recover(&state.shared.tab_manager);
        tab_manager.selected().map(|ws| ws.display_title().to_string())
    };
    let Some(current_title) = current_title else { return; };

    let dialog = adw::AlertDialog::builder()
        .heading("Rename Workspace")
        .close_response("cancel")
        .default_response("rename")
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("rename", "Rename");
    dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);

    let entry = gtk4::Entry::new();
    entry.set_text(&current_title);
    entry.set_activates_default(true);
    dialog.set_extra_child(Some(&entry));

    let s = state.clone();
    let lb = list_box.clone();
    let cb = content_box.clone();
    dialog.connect_response(None, move |_dialog, response| {
        if response == "rename" {
            let new_title = entry.text().to_string();
            if !new_title.is_empty() {
                let mut tab_manager = lock_or_recover(&s.shared.tab_manager);
                if let Some(ws) = tab_manager.selected_mut() {
                    ws.custom_title = Some(new_title);
                }
                drop(tab_manager);
                refresh_ui(&lb, &cb, &s);
            }
        }
    });

    dialog.present(Some(window));
}

/// Rebuild the content area from the current workspace layout.
pub fn rebuild_content(content_box: &gtk4::Box, state: &Rc<AppState>) {
    while let Some(child) = content_box.first_child() {
        content_box.remove(&child);
    }

    // Clone workspace data out of the lock so we don't hold it during
    // GTK widget construction (build_layout callbacks may re-acquire it).
    let workspace_data = {
        let tab_manager = lock_or_recover(&state.shared.tab_manager);
        tab_manager.selected().map(|ws| {
            (ws.id, ws.layout.clone(), ws.panels.clone(), ws.attention_panel_id)
        })
    };

    if let Some((id, layout, panels, attention_panel_id)) = workspace_data {
        let widget = split_view::build_layout(id, &layout, &panels, attention_panel_id, state);
        content_box.append(&widget);
    } else {
        let label = gtk4::Label::new(Some("No workspace selected"));
        label.add_css_class("dim-label");
        content_box.append(&label);
    }
}

fn refresh_ui(list_box: &gtk4::ListBox, content_box: &gtk4::Box, state: &Rc<AppState>) {
    state.prune_terminal_cache();
    sidebar::refresh_sidebar(list_box, state);
    rebuild_content(content_box, state);
}

fn bind_sidebar_selection(list_box: &gtk4::ListBox, content_box: &gtk4::Box, state: &Rc<AppState>) {
    let state = state.clone();
    let lb = list_box.clone();
    let content_box = content_box.clone();

    list_box.connect_row_selected(move |_list_box, row| {
        let Some(row) = row else {
            return;
        };

        let index = row.index();
        if index < 0 {
            return;
        }
        if select_workspace_by_index(&state, index as usize) {
            refresh_ui(&lb, &content_box, &state);
        }
    });
}

fn bind_shared_state_updates(
    list_box: &gtk4::ListBox,
    content_box: &gtk4::Box,
    state: &Rc<AppState>,
    mut ui_events: UnboundedReceiver<UiEvent>,
) {
    let state = state.clone();
    let list_box = list_box.clone();
    let content_box = content_box.clone();

    glib::MainContext::default().spawn_local(async move {
        while let Some(event) = ui_events.recv().await {
            let mut pending = Some(event);
            let mut needs_refresh = false;
            loop {
                let event = match pending.take() {
                    Some(event) => event,
                    None => match ui_events.try_recv() {
                        Ok(event) => event,
                        Err(_) => break,
                    },
                };

                match event {
                    UiEvent::Refresh => needs_refresh = true,
                    UiEvent::SendInput { panel_id, text } => {
                        let sent = state.send_input_to_panel(panel_id, &text);
                        if !sent {
                            tracing::warn!(
                                %panel_id,
                                "surface.send_input dropped because panel is not ready"
                            );
                        }
                    }
                    UiEvent::BrowserOpen { panel_id, url } => {
                        let webview = state.browser_for(panel_id, Some(&url));
                        webkit6::prelude::WebViewExt::load_uri(&webview, &url);
                        needs_refresh = true;
                    }
                    UiEvent::BrowserBack { panel_id } => {
                        if let Some(wv) = state.browser_cache.borrow().get(&panel_id) {
                            if webkit6::prelude::WebViewExt::can_go_back(wv) {
                                webkit6::prelude::WebViewExt::go_back(wv);
                            }
                        }
                    }
                    UiEvent::BrowserForward { panel_id } => {
                        if let Some(wv) = state.browser_cache.borrow().get(&panel_id) {
                            if webkit6::prelude::WebViewExt::can_go_forward(wv) {
                                webkit6::prelude::WebViewExt::go_forward(wv);
                            }
                        }
                    }
                    UiEvent::BrowserReload { panel_id } => {
                        if let Some(wv) = state.browser_cache.borrow().get(&panel_id) {
                            webkit6::prelude::WebViewExt::reload(wv);
                        }
                    }
                    UiEvent::BrowserDevTools { panel_id } => {
                        if let Some(wv) = state.browser_cache.borrow().get(&panel_id) {
                            if let Some(inspector) = webkit6::prelude::WebViewExt::inspector(wv) {
                                inspector.show();
                            }
                        }
                    }
                }
            }

            if needs_refresh {
                refresh_ui(&list_box, &content_box, &state);
            }
        }
    });
}

fn select_workspace_by_index(state: &Rc<AppState>, index: usize) -> bool {
    let (selected, already_selected, workspace_id) = {
        let mut tab_manager = lock_or_recover(&state.shared.tab_manager);
        let already_selected = tab_manager.selected_index() == Some(index);
        let selected = tab_manager.select(index);
        let workspace_id = tab_manager.get(index).map(|workspace| workspace.id);
        (selected, already_selected, workspace_id)
    };

    if !selected || already_selected {
        return false;
    }

    if let Some(workspace_id) = workspace_id {
        mark_workspace_read(state, workspace_id);
    }

    true
}

fn select_latest_unread(state: &Rc<AppState>) -> bool {
    let workspace_id = {
        let mut tab_manager = lock_or_recover(&state.shared.tab_manager);
        tab_manager.select_latest_unread()
    };

    let Some(workspace_id) = workspace_id else {
        return false;
    };

    mark_workspace_read(state, workspace_id);
    true
}

fn mark_workspace_read(state: &Rc<AppState>, workspace_id: uuid::Uuid) {
    lock_or_recover(&state.shared.notifications).mark_workspace_read(workspace_id);

    if let Some(workspace) =
        lock_or_recover(&state.shared.tab_manager).workspace_mut(workspace_id)
    {
        workspace.mark_notifications_read();
    }
}

fn install_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        "
        .workspace-row {
            border-radius: 10px;
        }

        .sidebar-notification {
            color: @accent_color;
            font-weight: 600;
        }

        .panel-shell {
            border: 1px solid rgba(127, 127, 127, 0.18);
            border-radius: 10px;
            padding: 3px;
        }

        .attention-panel {
            border: 2px solid #3584e4;
            background-color: rgba(53, 132, 228, 0.08);
        }
        ",
    );

    if let Some(display) = gdk4::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
