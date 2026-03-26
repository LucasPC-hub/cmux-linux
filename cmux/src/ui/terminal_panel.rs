//! Panel widgets — wraps VTE terminals and WebKit browsers in panel containers.

use std::rc::Rc;

use gtk4::prelude::*;
use vte4::prelude::*;
use webkit6::prelude::*;

use crate::app::AppState;
use crate::model::panel::{Panel, PanelType};

/// Create a GTK widget for a panel.
pub fn create_panel_widget(
    panel: &Panel,
    is_attention_source: bool,
    state: &Rc<AppState>,
) -> gtk4::Widget {
    match panel.panel_type {
        PanelType::Terminal => create_terminal_widget(panel, is_attention_source, state),
        PanelType::Browser => create_browser_widget(panel, is_attention_source, state),
    }
}

/// Create a terminal panel widget backed by VTE.
fn create_terminal_widget(
    panel: &Panel,
    is_attention_source: bool,
    state: &Rc<AppState>,
) -> gtk4::Widget {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.set_hexpand(true);
    container.set_vexpand(true);
    container.add_css_class("panel-shell");
    if is_attention_source {
        container.add_css_class("attention-panel");
    }

    let terminal = state.terminal_for(panel.id, panel.directory.as_deref());

    // Handle child exit
    {
        let state = Rc::clone(state);
        let panel_id = panel.id;
        terminal.connect_child_exited(move |_term, _status| {
            let _ = state.close_panel(panel_id, false);
        });
    }

    // Reparent if needed (terminal may have been in a previous container)
    if let Some(parent) = terminal.parent() {
        if let Ok(parent_box) = parent.downcast::<gtk4::Box>() {
            parent_box.remove(&terminal);
        }
    }

    container.append(&terminal);
    container.set_widget_name(&panel.id.to_string());
    container.upcast()
}

/// Create a browser panel widget backed by WebKitGTK.
fn create_browser_widget(
    panel: &Panel,
    is_attention_source: bool,
    state: &Rc<AppState>,
) -> gtk4::Widget {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.set_hexpand(true);
    container.set_vexpand(true);
    container.add_css_class("panel-shell");
    if is_attention_source {
        container.add_css_class("attention-panel");
    }

    // URL bar
    let url_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    url_bar.set_margin_start(4);
    url_bar.set_margin_end(4);
    url_bar.set_margin_top(4);
    url_bar.set_margin_bottom(4);

    let back_btn = gtk4::Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some("Back"));
    let forward_btn = gtk4::Button::from_icon_name("go-next-symbolic");
    forward_btn.set_tooltip_text(Some("Forward"));
    let reload_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
    reload_btn.set_tooltip_text(Some("Reload"));

    let url_entry = gtk4::Entry::new();
    url_entry.set_hexpand(true);
    url_entry.set_placeholder_text(Some("Enter URL..."));

    let devtools_btn = gtk4::Button::from_icon_name("applications-engineering-symbolic");
    devtools_btn.set_tooltip_text(Some("Developer Tools"));

    url_bar.append(&back_btn);
    url_bar.append(&forward_btn);
    url_bar.append(&reload_btn);
    url_bar.append(&url_entry);
    url_bar.append(&devtools_btn);

    // Create the WebView
    let default_url = panel.directory.as_deref(); // reuse directory field as initial URL
    let webview = state.browser_for(panel.id, default_url);

    // Reparent if needed
    if let Some(parent) = webview.parent() {
        if let Ok(parent_box) = parent.downcast::<gtk4::Box>() {
            parent_box.remove(&webview);
        }
    }

    // Update URL bar when navigation occurs
    {
        let entry = url_entry.clone();
        webkit6::prelude::WebViewExt::connect_uri_notify(&webview, move |wv: &webkit6::WebView| {
            let uri: Option<glib::GString> = webkit6::prelude::WebViewExt::uri(wv);
            if let Some(uri) = uri {
                entry.set_text(&uri);
            }
        });
    }

    // Navigate on Enter in URL bar
    {
        let wv = webview.clone();
        url_entry.connect_activate(move |entry| {
            let mut url = entry.text().to_string();
            if !url.is_empty() {
                if !url.contains("://") {
                    url = format!("https://{}", url);
                }
                webkit6::prelude::WebViewExt::load_uri(&wv, &url);
            }
        });
    }

    // Back/Forward/Reload buttons
    {
        let wv = webview.clone();
        back_btn.connect_clicked(move |_| {
            if webkit6::prelude::WebViewExt::can_go_back(&wv) {
                webkit6::prelude::WebViewExt::go_back(&wv);
            }
        });
    }
    {
        let wv = webview.clone();
        forward_btn.connect_clicked(move |_| {
            if webkit6::prelude::WebViewExt::can_go_forward(&wv) {
                webkit6::prelude::WebViewExt::go_forward(&wv);
            }
        });
    }
    {
        let wv = webview.clone();
        reload_btn.connect_clicked(move |_| {
            webkit6::prelude::WebViewExt::reload(&wv);
        });
    }

    // DevTools button
    {
        let wv = webview.clone();
        devtools_btn.connect_clicked(move |_| {
            if let Some(inspector) = webkit6::prelude::WebViewExt::inspector(&wv) {
                inspector.show();
            }
        });
    }

    container.append(&url_bar);
    container.append(&webview);
    container.set_widget_name(&panel.id.to_string());
    container.upcast()
}
