mod backend;
mod cloudflare;
mod db_restore;

use adw::gio;
use backend::{Manager, ServiceState};
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Label, ListBox, ListBoxRow, Orientation, ScrolledWindow,
    SearchEntry, Stack, TextView,
};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

struct RowWidgets {
    row: ListBoxRow,
    search_text: String,
    dot: Label,
    subtitle: Label,
}

struct DetailWidgets {
    page: adw::PreferencesPage,
    state_row: adw::ActionRow,
    pid_row: adw::ActionRow,
    start_btn: Button,
    stop_btn: Button,
    restart_btn: Button,
    autostart_row: adw::SwitchRow,
    status_banner: adw::Banner,
    source_row: adw::ActionRow,
    edit_row: adw::ActionRow,
    file_row: adw::ActionRow,
    command_row: adw::ActionRow,
    workdir_row: adw::ActionRow,
    unit_row: adw::ActionRow,
    database_group: adw::PreferencesGroup,
    danger_group: adw::PreferencesGroup,
    logs_view: TextView,
    logs_expander: adw::ExpanderRow,
}

fn selection_parts(selection: &str) -> Option<(&str, &str)> {
    selection.split_once(':')
}

fn main() {
    let app = adw::Application::builder()
        .application_id("io.github.hilalfajar.ServiceCenterVrust")
        .build();

    let about_action = gio::SimpleAction::new("about", None);
    let app_c = app.clone();
    about_action.connect_activate(move |_, _| {
        let dialog = adw::AboutDialog::builder()
            .application_name("Service Center")
            .developer_name("Hilal Fajar")
            .version("1.2.0")
            .comments(
                "Aplikasi GTK4 yang ditulis dengan Rust untuk mengelola layanan systemd pengguna.\n\n\
                 Daftarkan program secara manual sebagai layanan, atau lihat layanan yang sudah dimuat oleh systemd --user. \
                 Pantau status, jalankan, hentikan, atau restart layanan, dan baca log-nya.\n\n\
                 Layanan yang didaftarkan manual dapat dikelola dari Service Center. \
                 Layanan yang ditemukan otomatis hanya dapat dikontrol dan dilihat log-nya; \
                 konfigurasi, autostart, dan file unit-nya tidak diubah atau dihapus oleh aplikasi. \
                 Aplikasi ini mengelola layanan pengguna, bukan layanan system-wide.",
            )
            .build();
        if let Some(w) = app_c.windows().first() {
            dialog.present(Some(w));
        }
    });
    app.add_action(&about_action);

    let refresh_action = gio::SimpleAction::new("refresh", None);
    refresh_action.connect_activate(move |_, _| {
        crate::do_refresh();
    });
    app.add_action(&refresh_action);
    app.set_accels_for_action("app.refresh", &["<Primary>r"]);

    let quit_action = gio::SimpleAction::new("quit", None);
    let app_q = app.clone();
    quit_action.connect_activate(move |_, _| {
        app_q.quit();
    });
    app.add_action(&quit_action);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);

    app.connect_activate(|app| {
        if let Some(win) = app.windows().first() {
            win.present();
            return;
        }
        build_ui(app)
    });
    app.run();
}

thread_local! {
    static REFRESH_FN: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
}

fn do_refresh() {
    REFRESH_FN.with(|f| {
        if let Some(func) = f.borrow().as_ref() {
            func();
        }
    });
}

fn status_text(state: &ServiceState) -> &'static str {
    match state.active.as_str() {
        "active" => "Berjalan",
        "failed" => "Gagal",
        "activating" => "Menyalakan…",
        "deactivating" => "Menghentikan…",
        _ => "Berhenti",
    }
}

fn status_css(state: &ServiceState) -> &'static str {
    match state.active.as_str() {
        "active" => "dot-ok",
        "failed" => "dot-err",
        "activating" | "deactivating" => "dot-warn",
        _ => "dot-off",
    }
}

fn shell_join(cmd: &[String]) -> String {
    cmd.iter()
        .map(|p| {
            if !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "@%+=:,./_-".contains(c))
            {
                p.clone()
            } else {
                format!("'{}'", p.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_ui(app: &adw::Application) {
    gtk4::Window::set_default_icon_name("service-center");
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        ".dot-ok { color: #2ec27e; }\n\
         .dot-err { color: #e01b24; }\n\
         .dot-warn { color: #e5a50a; }\n\
         .dot-off { color: #8a8a8a; }\n\
         .service-name { font-weight: 600; }\n\
         .logs-view { font-family: monospace; font-size: 0.9em; }",
    );
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::StyleContext::add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let mgr = Rc::new(RefCell::new(Manager::new()));
    let system_services: Rc<RefCell<Vec<backend::SystemService>>> =
        Rc::new(RefCell::new(Vec::new()));
    let states: Rc<RefCell<HashMap<String, ServiceState>>> = Rc::new(RefCell::new(HashMap::new()));
    let current_id: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let rows: Rc<RefCell<HashMap<String, RowWidgets>>> = Rc::new(RefCell::new(HashMap::new()));
    let group_rows: Rc<RefCell<Vec<ListBoxRow>>> = Rc::new(RefCell::new(Vec::new()));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Service Center")
        .default_width(1060)
        .default_height(720)
        .build();

    let toast = adw::ToastOverlay::new();
    window.set_content(Some(&toast));

    let toolbar = adw::ToolbarView::new();
    toast.set_child(Some(&toolbar));

    let split = adw::NavigationSplitView::new();
    split.set_min_sidebar_width(320.0);
    split.set_max_sidebar_width(420.0);
    toolbar.set_content(Some(&split));

    // ---------------- Sidebar ----------------
    let sidebar = GtkBox::new(Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Service Center",
        "Pengelola layanan systemd",
    )));

    let add_btn = Button::from_icon_name("list-add-symbolic");
    add_btn.add_css_class("suggested-action");
    header.pack_start(&add_btn);

    let menu_btn = gtk4::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    let menu = gio::Menu::new();
    menu.append(Some("Muat ulang"), Some("app.refresh"));
    menu.append(Some("Cloudflare Tunnel…"), Some("win.cloudflare"));
    menu.append(Some("Tentang Service Center"), Some("app.about"));
    menu.append(Some("Keluar"), Some("app.quit"));
    menu_btn.set_menu_model(Some(&menu));
    header.pack_end(&menu_btn);
    sidebar.append(&header);

    let linger_banner =
        adw::Banner::new("Auto-start saat boot belum aktif. Layanan tidak jalan sebelum login");
    linger_banner.set_button_label(Some("Aktifkan"));
    linger_banner.set_revealed(!backend::linger_enabled());
    let linger_banner_c = linger_banner.clone();
    linger_banner.connect_button_clicked(move |_| {
        let (ok, _msg) = backend::enable_linger();
        if ok {
            linger_banner_c.set_revealed(false);
        }
    });
    sidebar.append(&linger_banner);

    let listbox = ListBox::new();
    listbox.set_selection_mode(gtk4::SelectionMode::Single);
    listbox.add_css_class("navigation-sidebar");

    let search = SearchEntry::new();
    search.set_placeholder_text(Some("Cari layanan…"));
    search.set_margin_top(6);
    search.set_margin_bottom(6);
    search.set_margin_start(8);
    search.set_margin_end(8);
    let rows_s = rows.clone();
    search.connect_search_changed(move |se| {
        let q = se.text().to_lowercase();
        for rw in rows_s.borrow().values() {
            rw.row.set_visible(rw.search_text.contains(&q));
        }
    });
    sidebar.append(&search);

    let discovery_status = Label::new(None);
    discovery_status.set_wrap(true);
    discovery_status.add_css_class("error");
    discovery_status.set_margin_start(12);
    discovery_status.set_margin_end(12);
    discovery_status.set_margin_bottom(6);
    discovery_status.set_visible(false);
    sidebar.append(&discovery_status);

    let scroller = ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroller.set_child(Some(&listbox));

    let empty_page = adw::StatusPage::new();
    empty_page.set_icon_name(Some("system-run-symbolic"));
    empty_page.set_title("Belum ada layanan");
    empty_page.set_description(Some(
        "Layanan systemd pengguna yang sudah dimuat akan ditemukan otomatis. Anda juga dapat mendaftarkan program secara manual.",
    ));
    let empty_add_btn = Button::with_label("Tambah Layanan");
    empty_add_btn.add_css_class("suggested-action");
    empty_add_btn.set_halign(Align::Center);
    empty_page.set_child(Some(&empty_add_btn));

    let sidebar_stack = Stack::new();
    sidebar_stack.set_vexpand(true);
    sidebar_stack.add_named(&scroller, Some("list"));
    sidebar_stack.add_named(&empty_page, Some("empty"));
    sidebar_stack.set_visible_child_name("empty");
    sidebar.append(&sidebar_stack);

    split.set_sidebar(Some(&adw::NavigationPage::new(&sidebar, "Layanan")));

    // ---------------- Content ----------------
    let content_toolbar = adw::ToolbarView::new();
    let content_header = adw::HeaderBar::new();
    content_header.set_title_widget(Some(&adw::WindowTitle::new("Detail layanan", "")));
    content_header.set_show_start_title_buttons(false);
    content_toolbar.add_top_bar(&content_header);

    let detail_stack = Stack::new();
    detail_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);

    let empty_detail = adw::StatusPage::new();
    empty_detail.set_icon_name(Some("utilities-system-monitor-symbolic"));
    empty_detail.set_title("Pilih layanan");
    empty_detail.set_description(Some(
        "Pilih layanan di kiri untuk melihat status, mengontrol, dan membaca log.",
    ));
    detail_stack.add_named(&empty_detail, Some("empty"));

    // ---- PreferencesPage detail ----
    let page = adw::PreferencesPage::new();

    // Status group
    let status_group = adw::PreferencesGroup::new();
    status_group.set_title("Status");
    let status_banner = adw::Banner::new("Layanan gagal. Cek log di bawah");
    status_banner.set_revealed(false);
    status_group.add(&status_banner);
    let state_row = adw::ActionRow::new();
    state_row.set_title("Keadaan");
    let pid_row = adw::ActionRow::new();
    pid_row.set_title("PID");
    status_group.add(&state_row);
    status_group.add(&pid_row);

    let controls = adw::ActionRow::new();
    controls.set_title("Kontrol");
    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.set_valign(Align::Center);
    let start_btn = Button::with_label("Mulai");
    start_btn.add_css_class("suggested-action");
    let stop_btn = Button::with_label("Hentikan");
    stop_btn.add_css_class("destructive-action");
    let restart_btn = Button::with_label("Restart");
    buttons.append(&start_btn);
    buttons.append(&stop_btn);
    buttons.append(&restart_btn);
    controls.add_suffix(&buttons);
    status_group.add(&controls);

    let autostart_row = adw::SwitchRow::new();
    autostart_row.set_title("Mulai otomatis");
    autostart_row.set_subtitle("Jalankan saat boot (perlu linger aktif)");
    status_group.add(&autostart_row);
    page.add(&status_group);

    // Detail group
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title("Detail");
    let edit_row = adw::ActionRow::new();
    edit_row.set_title("Edit layanan");
    edit_row.set_subtitle("Ubah nama, file, perintah, direktori, env, restart");
    let edit_btn = Button::with_label("Edit");
    edit_btn.set_valign(Align::Center);
    edit_row.add_suffix(&edit_btn);
    edit_row.set_activatable_widget(Some(&edit_btn));
    info_group.add(&edit_row);
    let source_row = adw::ActionRow::new();
    source_row.set_title("Sumber");
    info_group.add(&source_row);
    let file_row = adw::ActionRow::new();
    file_row.set_title("File");
    let command_row = adw::ActionRow::new();
    command_row.set_title("Perintah");
    let workdir_row = adw::ActionRow::new();
    workdir_row.set_title("Direktori");
    let unit_row = adw::ActionRow::new();
    unit_row.set_title("Unit");
    for row in [&file_row, &command_row, &workdir_row, &unit_row] {
        row.set_subtitle_selectable(true);
        info_group.add(row);
    }
    page.add(&info_group);

    // Database group
    let database_group = adw::PreferencesGroup::new();
    database_group.set_title("Database");
    let restore_row = adw::ActionRow::new();
    restore_row.set_title("Restore database");
    restore_row.set_subtitle("Pulihkan dump PostgreSQL/MySQL ke database aplikasi");
    let restore_btn = Button::with_label("Restore");
    restore_btn.set_valign(Align::Center);
    restore_row.add_suffix(&restore_btn);
    restore_row.set_activatable_widget(Some(&restore_btn));
    database_group.add(&restore_row);
    page.add(&database_group);

    // Log group
    let logs_group = adw::PreferencesGroup::new();
    logs_group.set_title("Log");
    let logs_expander = adw::ExpanderRow::new();
    logs_expander.set_title("Log layanan");
    logs_expander.set_subtitle("journalctl --user");
    let log_box = GtkBox::new(Orientation::Vertical, 6);
    log_box.set_margin_top(6);
    log_box.set_margin_bottom(6);
    log_box.set_margin_start(6);
    log_box.set_margin_end(6);
    let logs_view = TextView::new();
    logs_view.set_editable(false);
    logs_view.set_cursor_visible(false);
    logs_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    logs_view.set_left_margin(8);
    logs_view.set_right_margin(8);
    logs_view.set_top_margin(6);
    logs_view.set_bottom_margin(6);
    logs_view.add_css_class("logs-view");
    let log_scroller = ScrolledWindow::new();
    log_scroller.set_min_content_height(220);
    log_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    log_scroller.set_child(Some(&logs_view));
    log_box.append(&log_scroller);
    let reload_logs_btn = Button::with_label("Muat ulang log");
    reload_logs_btn.set_halign(Align::End);
    log_box.append(&reload_logs_btn);
    logs_expander.add_row(&log_box);
    logs_group.add(&logs_expander);
    page.add(&logs_group);

    // Danger group
    let danger_group = adw::PreferencesGroup::new();
    danger_group.set_title("Zona berbahaya");
    let delete_row = adw::ActionRow::new();
    delete_row.set_title("Hapus layanan");
    delete_row.set_subtitle("Hentikan dan hapus unit systemd (file program tidak disentuh)");
    let delete_btn = Button::from_icon_name("user-trash-symbolic");
    delete_btn.set_valign(Align::Center);
    delete_btn.add_css_class("destructive-action");
    delete_row.add_suffix(&delete_btn);
    delete_row.set_activatable_widget(Some(&delete_btn));
    danger_group.add(&delete_row);
    page.add(&danger_group);

    detail_stack.add_named(&page, Some("detail"));
    detail_stack.set_visible_child_name("empty");

    content_toolbar.set_content(Some(&detail_stack));
    split.set_content(Some(&adw::NavigationPage::new(&content_toolbar, "Detail")));

    let detail = Rc::new(DetailWidgets {
        page: page.clone(),
        state_row: state_row.clone(),
        pid_row: pid_row.clone(),
        start_btn: start_btn.clone(),
        stop_btn: stop_btn.clone(),
        restart_btn: restart_btn.clone(),
        autostart_row: autostart_row.clone(),
        status_banner: status_banner.clone(),
        source_row: source_row.clone(),
        edit_row: edit_row.clone(),
        file_row: file_row.clone(),
        command_row: command_row.clone(),
        workdir_row: workdir_row.clone(),
        unit_row: unit_row.clone(),
        database_group: database_group.clone(),
        danger_group: danger_group.clone(),
        logs_view: logs_view.clone(),
        logs_expander: logs_expander.clone(),
    });

    // ---------------- Row building ----------------
    let rebuild_rows = {
        let listbox = listbox.clone();
        let mgr = mgr.clone();
        let system_services = system_services.clone();
        let rows = rows.clone();
        let group_rows = group_rows.clone();
        let sidebar_stack = sidebar_stack.clone();
        let search = search.clone();
        let current_id = current_id.clone();
        move || {
            let selected_key = current_id.borrow().clone();
            for rw in rows.borrow().values() {
                listbox.remove(&rw.row);
            }
            for row in group_rows.borrow().iter() {
                listbox.remove(row);
            }
            rows.borrow_mut().clear();
            group_rows.borrow_mut().clear();

            let services = mgr.borrow().services();
            let system_services = system_services.borrow().clone();
            let query = search.text().to_lowercase();
            let add_group = |title: String| {
                let row = ListBoxRow::new();
                row.set_selectable(false);
                row.set_activatable(false);
                let label = Label::new(Some(&title));
                label.set_xalign(0.0);
                label.add_css_class("heading");
                label.set_margin_start(12);
                label.set_margin_top(10);
                label.set_margin_bottom(4);
                row.set_child(Some(&label));
                listbox.append(&row);
                group_rows.borrow_mut().push(row);
            };
            add_group(format!("Didaftarkan manual ({})", services.len()));
            for svc in &services {
                let row = ListBoxRow::new();
                let boxl = GtkBox::new(Orientation::Horizontal, 10);
                boxl.set_margin_start(12);
                boxl.set_margin_end(12);
                boxl.set_margin_top(8);
                boxl.set_margin_bottom(8);
                let dot = Label::new(Some("●"));
                dot.add_css_class("dot-off");
                dot.set_valign(Align::Center);
                let text_box = GtkBox::new(Orientation::Vertical, 2);
                text_box.set_hexpand(true);
                let name = Label::new(Some(&svc.name));
                name.set_xalign(0.0);
                name.add_css_class("service-name");
                name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                let subtitle = Label::new(Some("Berhenti"));
                subtitle.set_xalign(0.0);
                subtitle.add_css_class("caption");
                subtitle.add_css_class("dim-label");
                subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                text_box.append(&name);
                text_box.append(&subtitle);
                boxl.append(&dot);
                boxl.append(&text_box);
                row.set_child(Some(&boxl));
                let key = format!("manual:{}", svc.id);
                unsafe {
                    row.set_data("sid", key.clone());
                }
                row.set_visible(svc.name.to_lowercase().contains(&query));
                listbox.append(&row);
                rows.borrow_mut().insert(
                    key,
                    RowWidgets {
                        row,
                        search_text: svc.name.to_lowercase(),
                        dot,
                        subtitle,
                    },
                );
            }

            add_group(format!(
                "Layanan systemd pengguna ({})",
                system_services.len()
            ));
            for service in &system_services {
                let row = ListBoxRow::new();
                let boxl = GtkBox::new(Orientation::Horizontal, 10);
                boxl.set_margin_start(12);
                boxl.set_margin_end(12);
                boxl.set_margin_top(8);
                boxl.set_margin_bottom(8);
                let dot = Label::new(Some("●"));
                dot.add_css_class("dot-off");
                dot.set_valign(Align::Center);
                let text_box = GtkBox::new(Orientation::Vertical, 2);
                text_box.set_hexpand(true);
                let name = Label::new(Some(&service.description));
                name.set_xalign(0.0);
                name.add_css_class("service-name");
                name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                let subtitle = Label::new(Some(&service.unit));
                subtitle.set_xalign(0.0);
                subtitle.add_css_class("caption");
                subtitle.add_css_class("dim-label");
                subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                text_box.append(&name);
                text_box.append(&subtitle);
                boxl.append(&dot);
                boxl.append(&text_box);
                row.set_child(Some(&boxl));
                let key = format!("system:{}", service.unit);
                unsafe {
                    row.set_data("sid", key.clone());
                }
                let searchable = format!("{} {}", service.description, service.unit).to_lowercase();
                row.set_visible(searchable.contains(&query));
                listbox.append(&row);
                rows.borrow_mut().insert(
                    key,
                    RowWidgets {
                        row,
                        search_text: searchable,
                        dot,
                        subtitle,
                    },
                );
            }
            let has_services = !services.is_empty() || !system_services.is_empty();
            sidebar_stack.set_visible_child_name(if has_services { "list" } else { "empty" });
            if let Some(key) = selected_key.as_ref() {
                if let Some(row) = rows.borrow().get(key) {
                    listbox.select_row(Some(&row.row));
                }
            }
        }
    };
    rebuild_rows();
    let rebuild_rows_rc: Rc<dyn Fn()> = Rc::new(rebuild_rows);

    // ---------------- Cloudflare Tunnel ----------------
    {
        let mgr = mgr.clone();
        let toast = toast.clone();
        let rebuild_rows = rebuild_rows_rc.clone();
        let window_d = window.clone();
        let action = gio::SimpleAction::new("cloudflare", None);
        action.connect_activate(move |_, _| {
            open_cloudflare_dialog(&window_d, &mgr, &toast, rebuild_rows.clone());
        });
        window.add_action(&action);
    }

    // ---------------- Selection ----------------
    let detail_sel = detail.clone();
    let mgr_sel = mgr.clone();
    let system_services_sel = system_services.clone();
    let states_sel = states.clone();
    let current_sel = current_id.clone();
    let detail_stack_sel = detail_stack.clone();
    let rows_sel = rows.clone();
    let updating = Rc::new(RefCell::new(false));
    let updating_sel = updating.clone();
    listbox.connect_row_selected(move |_lb, row| {
        let Some(row) = row else {
            *current_sel.borrow_mut() = None;
            detail_stack_sel.set_visible_child_name("empty");
            return;
        };
        let sid = unsafe {
            row.data::<String>("sid")
                .map(|d| d.as_ref().clone())
                .unwrap_or_default()
        };
        if sid.is_empty() {
            return;
        }
        *current_sel.borrow_mut() = Some(sid.clone());
        detail_stack_sel.set_visible_child_name("detail");
        let _ = &rows_sel;
        update_detail(
            &mgr_sel,
            &system_services_sel,
            &states_sel,
            &current_sel,
            &detail_sel,
            &updating_sel,
        );
        load_logs(&mgr_sel, &current_sel, &detail_sel);
    });

    // ---------------- Actions ----------------
    let make_action = |verb: &'static str| {
        let mgr = mgr.clone();
        let current = current_id.clone();
        let toast = toast.clone();
        move |_btn: &Button| {
            if let Some(selection) = current.borrow().clone() {
                let result = match selection_parts(&selection) {
                    Some(("manual", sid)) => mgr.borrow().action(sid, verb),
                    Some(("system", unit)) => mgr.borrow().system_action(unit, verb),
                    _ => return,
                };
                match result {
                    Ok((ok, msg)) => {
                        if !ok {
                            toast.add_toast(adw::Toast::new(&if msg.is_empty() {
                                "Perintah gagal".to_string()
                            } else {
                                msg
                            }));
                        }
                    }
                    Err(e) => {
                        toast.add_toast(adw::Toast::new(&format!("Gagal: {}", e)));
                    }
                }
            }
        }
    };
    start_btn.connect_clicked(make_action("start"));
    stop_btn.connect_clicked(make_action("stop"));
    restart_btn.connect_clicked(make_action("restart"));

    // Autostart toggle
    let mgr_auto = mgr.clone();
    let current_auto = current_id.clone();
    let toast_auto = toast.clone();
    let updating_set = updating.clone();
    autostart_row.connect_active_notify(move |row| {
        if *updating_set.borrow() {
            return;
        }
        if let Some(sid) = current_auto.borrow().clone() {
            let Some(("manual", sid)) = selection_parts(&sid) else {
                return;
            };
            let enabled = row.is_active();
            match mgr_auto.borrow().set_enabled(&sid, enabled) {
                Ok((ok, msg)) => {
                    if !ok {
                        toast_auto.add_toast(adw::Toast::new(&if msg.is_empty() {
                            "Gagal mengubah autostart".to_string()
                        } else {
                            msg
                        }));
                    }
                }
                Err(e) => {
                    toast_auto.add_toast(adw::Toast::new(&e));
                }
            }
        }
    });

    // Logs expander
    let mgr_log = mgr.clone();
    let current_log = current_id.clone();
    let detail_log = detail.clone();
    logs_expander.connect_expanded_notify(move |exp| {
        if exp.is_expanded() {
            load_logs(&mgr_log, &current_log, &detail_log);
        }
    });
    let mgr_reload = mgr.clone();
    let current_reload = current_id.clone();
    let detail_reload = detail.clone();
    reload_logs_btn.connect_clicked(move |_| {
        load_logs(&mgr_reload, &current_reload, &detail_reload);
    });

    // Edit
    {
        let window = window.clone();
        let mgr = mgr.clone();
        let toast = toast.clone();
        let current = current_id.clone();
        let rebuild_rows = rebuild_rows_rc.clone();
        edit_btn.connect_clicked(move |_| {
            let Some(sid) = current.borrow().clone() else {
                return;
            };
            let Some(("manual", sid)) = selection_parts(&sid) else {
                return;
            };
            open_edit_dialog(&window, &mgr, &toast, sid, rebuild_rows.clone());
        });
    }

    // Delete
    {
        let mgr = mgr.clone();
        let current = current_id.clone();
        let toast = toast.clone();
        let window = window.clone();
        let rebuild_rows = rebuild_rows_rc.clone();
        let rebuild_rows_c = rebuild_rows.clone();
        let detail_stack_d = detail_stack.clone();
        delete_btn.connect_clicked(move |_| {
            let Some(sid) = current.borrow().clone() else {
                return;
            };
            if !matches!(selection_parts(&sid), Some(("manual", _))) {
                return;
            }
            let dialog = adw::AlertDialog::new(
                Some("Hapus layanan?"),
                Some("Layanan akan dihentikan dan unit systemd dihapus."),
            );
            dialog.add_response("cancel", "Batal");
            dialog.add_response("delete", "Hapus");
            dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));
            let mgr = mgr.clone();
            let toast = toast.clone();
            let current = current.clone();
            let rebuild_rows = rebuild_rows_c.clone();
            let detail_stack = detail_stack_d.clone();
            let sid_move = sid.clone();
            dialog.connect_response(None, move |_d, resp| {
                if resp == "delete" {
                    if let Some(("manual", service_id)) = selection_parts(&sid_move) {
                        let _ = mgr.borrow_mut().delete(service_id);
                    }
                    *current.borrow_mut() = None;
                    detail_stack.set_visible_child_name("empty");
                    rebuild_rows();
                    toast.add_toast(adw::Toast::new("Layanan dihapus"));
                }
            });
            dialog.present(Some(&window));
        });
    }

    // Add button + empty button
    {
        let open = {
            let window = window.clone();
            let mgr = mgr.clone();
            let toast = toast.clone();
            let rebuild_rows = rebuild_rows_rc.clone();
            Rc::new(move || {
                open_add_dialog(&window, &mgr, &toast, rebuild_rows.clone());
            })
        };
        let oa = open.clone();
        add_btn.connect_clicked(move |_| oa());
        let oa2 = open.clone();
        empty_add_btn.connect_clicked(move |_| oa2());
    }

    // Restore dialog
    {
        let window = window.clone();
        let mgr = mgr.clone();
        let toast = toast.clone();
        let current = current_id.clone();
        restore_btn.connect_clicked(move |_| {
            let Some(sid) = current.borrow().clone() else {
                return;
            };
            let Some(("manual", service_id)) = selection_parts(&sid) else {
                return;
            };
            let svc = mgr.borrow().get(service_id);
            if let Some(svc) = svc {
                open_restore_dialog(&window, &mgr, &toast, &svc.id, &svc.name);
            }
        });
    }

    // ---------------- Refresh timer ----------------
    let mgr_t = mgr.clone();
    let system_services_t = system_services.clone();
    let states_t = states.clone();
    let rows_t = rows.clone();
    let current_t = current_id.clone();
    let detail_t = detail.clone();
    let updating_t = updating.clone();
    let rebuild_rows_t = rebuild_rows_rc.clone();
    let discovery_status_t = discovery_status.clone();
    adw::glib::timeout_add_seconds_local(2, move || {
        refresh(
            &mgr_t,
            &system_services_t,
            &states_t,
            &rows_t,
            &current_t,
            &detail_t,
            &updating_t,
            &rebuild_rows_t,
            &discovery_status_t,
        );
        adw::glib::ControlFlow::Continue
    });

    refresh(
        &mgr,
        &system_services,
        &states,
        &rows,
        &current_id,
        &detail,
        &updating,
        &rebuild_rows_rc,
        &discovery_status,
    );

    {
        let mgr = mgr.clone();
        let system_services = system_services.clone();
        let states = states.clone();
        let rows = rows.clone();
        let current = current_id.clone();
        let detail = detail.clone();
        let updating = updating.clone();
        let rebuild_rows = rebuild_rows_rc.clone();
        let discovery_status = discovery_status.clone();
        REFRESH_FN.with(|f| {
            *f.borrow_mut() = Some(Rc::new(move || {
                refresh(
                    &mgr,
                    &system_services,
                    &states,
                    &rows,
                    &current,
                    &detail,
                    &updating,
                    &rebuild_rows,
                    &discovery_status,
                );
            }));
        });
    }

    window.present();
}

fn refresh(
    mgr: &Rc<RefCell<Manager>>,
    system_services: &Rc<RefCell<Vec<backend::SystemService>>>,
    states: &Rc<RefCell<HashMap<String, ServiceState>>>,
    rows: &Rc<RefCell<HashMap<String, RowWidgets>>>,
    current: &Rc<RefCell<Option<String>>>,
    detail: &Rc<DetailWidgets>,
    updating: &Rc<RefCell<bool>>,
    rebuild_rows: &Rc<dyn Fn()>,
    discovery_status: &Label,
) {
    let services = mgr.borrow().services();
    match mgr.borrow().system_services() {
        Ok(discovered) => {
            let old_units: Vec<String> = system_services
                .borrow()
                .iter()
                .map(|service| service.unit.clone())
                .collect();
            let new_units: Vec<String> = discovered
                .iter()
                .map(|service| service.unit.clone())
                .collect();
            discovery_status.set_visible(false);
            if old_units != new_units {
                *system_services.borrow_mut() = discovered;
                rebuild_rows();
            }
        }
        Err(error) => {
            discovery_status.set_text(&format!(
                "Tidak dapat membaca layanan systemd pengguna: {}",
                error
            ));
            discovery_status.set_visible(true);
        }
    }
    let mut units: Vec<String> = services.iter().map(|s| s.unit_name()).collect();
    units.extend(
        system_services
            .borrow()
            .iter()
            .map(|service| service.unit.clone()),
    );
    let st = mgr.borrow().states(Some(units));
    let mut map = HashMap::new();
    for (unit, s) in st {
        map.insert(unit, s);
    }
    *states.borrow_mut() = map;

    for (key, unit, missing) in services
        .iter()
        .map(|service| {
            (
                format!("manual:{}", service.id),
                service.unit_name(),
                !mgr.borrow().unit_exists(&service.id),
            )
        })
        .chain(system_services.borrow().iter().map(|service| {
            (
                format!("system:{}", service.unit),
                service.unit.clone(),
                !states.borrow().contains_key(&service.unit),
            )
        }))
    {
        let state = states.borrow().get(&unit).cloned().unwrap_or_default();
        if let Some(rw) = rows.borrow().get(&key) {
            for c in ["dot-ok", "dot-err", "dot-warn", "dot-off"] {
                rw.dot.remove_css_class(c);
            }
            if missing {
                rw.dot.add_css_class("dot-err");
                rw.subtitle.set_text("Unit tidak dimuat");
            } else {
                rw.dot.add_css_class(status_css(&state));
                let suffix = if key.starts_with("manual:") && state.enabled == "enabled" {
                    " · autostart"
                } else {
                    ""
                };
                rw.subtitle
                    .set_text(&format!("{}{}", status_text(&state), suffix));
            }
        }
    }

    update_detail(mgr, system_services, states, current, detail, updating);
}

fn update_detail(
    mgr: &Rc<RefCell<Manager>>,
    system_services: &Rc<RefCell<Vec<backend::SystemService>>>,
    states: &Rc<RefCell<HashMap<String, ServiceState>>>,
    current: &Rc<RefCell<Option<String>>>,
    detail: &Rc<DetailWidgets>,
    updating: &Rc<RefCell<bool>>,
) {
    let Some(selection) = current.borrow().clone() else {
        return;
    };
    let Some((kind, id)) = selection_parts(&selection) else {
        return;
    };
    let (name, unit, file, command, workdir, is_manual) = match kind {
        "manual" => {
            let Some(service) = mgr.borrow().get(id) else {
                return;
            };
            let unit = service.unit_name();
            (
                service.name,
                unit,
                service.file,
                shell_join(&service.command),
                service.workdir,
                true,
            )
        }
        "system" => {
            let Some(service) = system_services
                .borrow()
                .iter()
                .find(|service| service.unit == id)
                .cloned()
            else {
                return;
            };
            let state = states.borrow().get(id).cloned().unwrap_or_default();
            (
                service.description,
                service.unit,
                state.fragment_path,
                state.exec_start,
                "Tidak tersedia".into(),
                false,
            )
        }
        _ => return,
    };
    let state = states.borrow().get(&unit).cloned().unwrap_or_default();
    let unit_missing = if is_manual {
        !mgr.borrow().unit_exists(id)
    } else {
        !states.borrow().contains_key(&unit)
    };

    let source = if is_manual {
        "Didaftarkan manual di Service Center".to_string()
    } else {
        format!(
            "{} · ditemukan dari systemd --user; unit tidak diubah oleh aplikasi",
            name
        )
    };
    detail.source_row.set_subtitle(&source);
    if unit_missing {
        detail.state_row.set_subtitle("Unit tidak dimuat");
        detail
            .status_banner
            .set_title("Unit systemd tidak ditemukan atau tidak dapat dibaca");
    } else {
        let mut text = status_text(&state).to_string();
        if !state.sub.is_empty() {
            text = format!("{} ({})", text, state.sub);
        }
        detail.state_row.set_subtitle(&text);
    }
    detail
        .status_banner
        .set_revealed(state.failed() || unit_missing);
    let pid = if state.pid.is_empty() || state.pid == "0" {
        "-"
    } else {
        &state.pid
    };
    detail.pid_row.set_subtitle(pid);

    detail.file_row.set_title(if is_manual {
        "File program"
    } else {
        "File unit"
    });
    detail.file_row.set_subtitle(if file.is_empty() {
        "Tidak tersedia"
    } else {
        &file
    });
    detail.command_row.set_subtitle(if command.is_empty() {
        "Tidak tersedia"
    } else {
        &command
    });
    detail.workdir_row.set_subtitle(&workdir);
    detail.unit_row.set_subtitle(&unit);

    let running = state.running();
    detail.start_btn.set_sensitive(!running && !unit_missing);
    detail.stop_btn.set_sensitive(running && !unit_missing);
    detail.restart_btn.set_sensitive(running && !unit_missing);
    detail.autostart_row.set_visible(is_manual);
    detail.edit_row.set_visible(is_manual);
    detail.database_group.set_visible(is_manual);
    detail.danger_group.set_visible(is_manual);
    detail
        .autostart_row
        .set_sensitive(is_manual && !unit_missing);
    let desired = state.enabled == "enabled";
    if detail.autostart_row.is_active() != desired {
        *updating.borrow_mut() = true;
        detail.autostart_row.set_active(desired);
        *updating.borrow_mut() = false;
    }
}

fn load_logs(
    mgr: &Rc<RefCell<Manager>>,
    current: &Rc<RefCell<Option<String>>>,
    detail: &Rc<DetailWidgets>,
) {
    let Some(selection) = current.borrow().clone() else {
        return;
    };
    let Some((kind, id)) = selection_parts(&selection) else {
        return;
    };
    let buf = detail.logs_view.buffer();
    let result = match kind {
        "manual" => mgr.borrow().logs(id, 200),
        "system" => mgr.borrow().system_logs(id, 200),
        _ => return,
    };
    match result {
        Ok((ok, text)) => {
            if ok {
                buf.set_text(if text.trim().is_empty() {
                    "Belum ada log."
                } else {
                    &text
                });
            } else {
                buf.set_text(&text);
            }
        }
        Err(e) => buf.set_text(&format!("Gagal memuat log:\n{}", e)),
    }
}

fn open_restore_dialog(
    window: &adw::ApplicationWindow,
    mgr: &Rc<RefCell<Manager>>,
    toast: &adw::ToastOverlay,
    service_id: &str,
    service_name: &str,
) {
    let service_id = service_id.to_string();
    let service_name = service_name.to_string();

    let dialog = adw::Dialog::new();
    dialog.set_content_width(640);
    dialog.set_content_height(780);
    dialog.set_title("Restore Database");

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Restore Database",
        &service_name,
    )));
    let close_btn = Button::with_label("Tutup");
    {
        let d = dialog.clone();
        close_btn.connect_clicked(move |_| {
            d.close();
        });
    }
    header.pack_start(&close_btn);
    let start_btn = Button::with_label("Start");
    start_btn.add_css_class("suggested-action");
    header.pack_end(&start_btn);
    toolbar.add_top_bar(&header);
    dialog.set_child(Some(&toolbar));

    let body = GtkBox::new(Orientation::Vertical, 0);
    let banner = adw::Banner::new("");
    banner.set_revealed(false);
    let page = adw::PreferencesPage::new();
    body.append(&banner);
    body.append(&page);
    toolbar.set_content(Some(&body));

    // ---- Database ----
    let db_group = adw::PreferencesGroup::new();
    db_group.set_title("Database");
    db_group.set_description(Some("Mendukung PostgreSQL dan MySQL/MariaDB."));
    let engine_row = adw::ComboRow::new();
    engine_row.set_title("Jenis database");
    let labels: Vec<&str> = db_restore::ENGINE_LABELS.iter().map(|(_, l)| *l).collect();
    engine_row.set_model(Some(&gtk4::StringList::new(&labels)));
    engine_row.set_selected(0);
    db_group.add(&engine_row);

    let host_row = adw::EntryRow::new();
    host_row.set_title("Host");
    host_row.set_text("localhost");
    let port_row = adw::EntryRow::new();
    port_row.set_title("Port");
    port_row.set_text("5432");
    let user_row = adw::EntryRow::new();
    user_row.set_title("User");
    user_row.set_text("postgres");
    let pass_row = adw::ActionRow::new();
    pass_row.set_title("Password");
    let pass_entry = gtk4::PasswordEntry::new();
    pass_entry.set_show_peek_icon(true);
    pass_entry.set_hexpand(true);
    pass_entry.set_size_request(260, -1);
    pass_row.add_suffix(&pass_entry);
    pass_row.set_activatable_widget(Some(&pass_entry));
    let database_row = adw::EntryRow::new();
    database_row.set_title("Nama database");
    db_group.add(&host_row);
    db_group.add(&port_row);
    db_group.add(&user_row);
    db_group.add(&pass_row);
    db_group.add(&database_row);
    page.add(&db_group);

    // ---- File dump ----
    let file_group = adw::PreferencesGroup::new();
    file_group.set_title("File dump");
    file_group.set_description(Some("Format .sql, .dump, .backup, atau .tar."));
    let file_row = adw::ActionRow::new();
    file_row.set_title("File");
    file_row.set_subtitle("Belum dipilih");
    file_row.set_subtitle_selectable(true);
    let choose_btn = Button::with_label("Pilih File…");
    choose_btn.set_valign(Align::Center);
    file_row.add_suffix(&choose_btn);
    file_row.set_activatable_widget(Some(&choose_btn));
    file_group.add(&file_row);
    page.add(&file_group);

    // ---- Opsi ----
    let option_group = adw::PreferencesGroup::new();
    option_group.set_title("Opsi");
    let create_row = adw::SwitchRow::new();
    create_row.set_title("Buat database jika belum ada");
    create_row.set_active(true);
    let clean_row = adw::SwitchRow::new();
    clean_row.set_title("Kosongkan objek lama");
    clean_row.set_subtitle("Hapus skema public sebelum restore");
    let stop_row = adw::SwitchRow::new();
    stop_row.set_title("Hentikan layanan saat restore");
    stop_row.set_subtitle("Layanan dinyalakan lagi setelah restore selesai");
    stop_row.set_active(true);
    option_group.add(&create_row);
    option_group.add(&clean_row);
    option_group.add(&stop_row);
    page.add(&option_group);

    // ---- Log ----
    let log_group = adw::PreferencesGroup::new();
    log_group.set_title("Log");
    let log_expander = adw::ExpanderRow::new();
    log_expander.set_title("Log restore");
    log_expander.set_subtitle("Keluaran psql");
    log_expander.set_expanded(true);
    let log_box = GtkBox::new(Orientation::Vertical, 6);
    log_box.set_margin_top(6);
    log_box.set_margin_bottom(6);
    log_box.set_margin_start(6);
    log_box.set_margin_end(6);
    let status_label = Label::new(Some("Siap"));
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.add_css_class("dim-label");
    let log_view = TextView::new();
    log_view.set_editable(false);
    log_view.set_cursor_visible(false);
    log_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    log_view.set_left_margin(8);
    log_view.set_right_margin(8);
    log_view.set_top_margin(6);
    log_view.set_bottom_margin(6);
    log_view.add_css_class("logs-view");
    let log_scroller = ScrolledWindow::new();
    log_scroller.set_min_content_height(220);
    log_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    log_scroller.set_child(Some(&log_view));
    log_box.append(&status_label);
    log_box.append(&log_scroller);
    log_expander.add_row(&log_box);
    log_group.add(&log_expander);
    page.add(&log_group);

    // ---- Ganti engine -> sesuaikan default ----
    {
        let port_row = port_row.clone();
        let user_row = user_row.clone();
        let clean_row = clean_row.clone();
        let file_group = file_group.clone();
        let log_expander = log_expander.clone();
        let banner = banner.clone();
        engine_row.connect_selected_notify(move |row| {
            let engine = if row.selected() == 1 {
                "mysql"
            } else {
                "postgresql"
            };
            let port = port_row.text().to_string();
            if port.trim().is_empty() || port == "5432" || port == "3306" {
                port_row.set_text(db_restore::default_port(engine));
            }
            let user = user_row.text().to_string();
            if user.trim().is_empty() || user == "postgres" || user == "root" {
                user_row.set_text(if engine == "mysql" {
                    "root"
                } else {
                    "postgres"
                });
            }
            if engine == "mysql" {
                clean_row.set_subtitle("Hapus dan buat ulang database sebelum restore");
                file_group.set_description(Some("MySQL/MariaDB hanya mendukung file .sql."));
                log_expander.set_subtitle("Keluaran client MySQL");
            } else {
                clean_row.set_subtitle("Hapus skema public sebelum restore");
                file_group.set_description(Some("Format .sql, .dump, .backup, atau .tar."));
                log_expander.set_subtitle("Keluaran psql");
            }
            banner.set_revealed(false);
        });
    }

    // ---- Pilih dump ----
    let dump_path: Rc<RefCell<Option<std::path::PathBuf>>> = Rc::new(RefCell::new(None));
    {
        let dump_path = dump_path.clone();
        let file_row = file_row.clone();
        let banner = banner.clone();
        let engine_row = engine_row.clone();
        let window_c = window.clone();
        choose_btn.connect_clicked(move |_| {
            let mysql = engine_row.selected() == 1;
            let dump_path = dump_path.clone();
            let file_row = file_row.clone();
            let banner = banner.clone();
            if mysql {
                pick_file_filtered(
                    &window_c,
                    "Pilih file dump",
                    "Dump MySQL (*.sql)",
                    &["*.sql"],
                    move |path| {
                        file_row.set_subtitle(&path.to_string_lossy());
                        *dump_path.borrow_mut() = Some(path);
                        banner.set_revealed(false);
                    },
                );
            } else {
                pick_file_filtered(
                    &window_c,
                    "Pilih file dump",
                    "Dump database (*.sql, *.dump, *.backup, *.tar)",
                    &["*.sql", "*.dump", "*.backup", "*.tar"],
                    move |path| {
                        file_row.set_subtitle(&path.to_string_lossy());
                        *dump_path.borrow_mut() = Some(path);
                        banner.set_revealed(false);
                    },
                );
            }
        });
    }

    // ---- Log helper ----
    let append_log: Rc<dyn Fn(&str)> = {
        let view = log_view.clone();
        Rc::new(move |text: &str| {
            let buf = view.buffer();
            let mut end = buf.end_iter();
            if buf.char_count() > 0 {
                buf.insert(&mut end, "\n");
            }
            buf.insert(&mut end, text);
            let mut end = buf.end_iter();
            view.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
        })
    };

    // ---- Aktif/nonaktif form saat berjalan ----
    let set_running: Rc<dyn Fn(bool)> = {
        let widgets: Vec<gtk4::Widget> = vec![
            engine_row.clone().upcast(),
            host_row.clone().upcast(),
            port_row.clone().upcast(),
            user_row.clone().upcast(),
            pass_row.clone().upcast(),
            database_row.clone().upcast(),
            file_row.clone().upcast(),
            create_row.clone().upcast(),
            clean_row.clone().upcast(),
            stop_row.clone().upcast(),
        ];
        let start_btn = start_btn.clone();
        let close_btn = close_btn.clone();
        let dialog = dialog.clone();
        Rc::new(move |run: bool| {
            for w in &widgets {
                w.set_sensitive(!run);
            }
            start_btn.set_sensitive(!run);
            close_btn.set_sensitive(!run);
            dialog.set_can_close(!run);
        })
    };

    let busy = Rc::new(std::cell::Cell::new(false));
    let logs: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let finished: std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));

    // ---- Mulai ----
    {
        let mgr = mgr.clone();
        let toast = toast.clone();
        let busy = busy.clone();
        let logs = logs.clone();
        let finished = finished.clone();
        let append_log = append_log.clone();
        let set_running = set_running.clone();
        let banner = banner.clone();
        let engine_row = engine_row.clone();
        let host_row = host_row.clone();
        let port_row = port_row.clone();
        let user_row = user_row.clone();
        let pass_entry = pass_entry.clone();
        let database_row = database_row.clone();
        let create_row = create_row.clone();
        let clean_row = clean_row.clone();
        let stop_row = stop_row.clone();
        let dump_path = dump_path.clone();
        let status_label = status_label.clone();
        let log_expander = log_expander.clone();
        let log_view = log_view.clone();
        let service_id = service_id.clone();
        let service_name = service_name.clone();

        start_btn.connect_clicked(move |_| {
            if busy.get() {
                return;
            }
            banner.set_revealed(false);

            let Some(dump) = dump_path.borrow().clone() else {
                banner.set_title("Pilih file dump dulu.");
                banner.set_revealed(true);
                return;
            };
            let engine = if engine_row.selected() == 1 {
                "mysql"
            } else {
                "postgresql"
            };
            if engine == "mysql" {
                let ext = dump
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if ext != "sql" {
                    banner.set_title("MySQL hanya mendukung dump .sql");
                    banner.set_revealed(true);
                    return;
                }
            }
            let req = db_restore::RestoreRequest {
                engine: engine.to_string(),
                host: host_row.text().trim().to_string(),
                port: port_row.text().trim().to_string(),
                user: user_row.text().trim().to_string(),
                password: pass_entry.text().to_string(),
                database: database_row.text().trim().to_string(),
                dump: dump.clone(),
                create_database: create_row.is_active(),
                clean: clean_row.is_active(),
            };
            if let Err(e) = req.validate() {
                banner.set_title(&e.0);
                banner.set_revealed(true);
                return;
            }

            log_view.buffer().set_text("");
            append_log(&format!("Jenis: {}", db_restore::engine_label(engine)));
            append_log(&format!(
                "Target: {}@{}:{}/{}",
                req.user, req.host, req.port, req.database
            ));
            append_log(&format!("File: {}", req.dump.display()));
            status_label.set_text("Berjalan…");
            busy.set(true);
            set_running(true);
            log_expander.set_expanded(true);
            log_view.grab_focus();

            let unit = mgr
                .borrow()
                .get(&service_id)
                .map(|s| s.unit_name())
                .unwrap_or_default();
            let was_running = mgr
                .borrow()
                .states(Some(vec![unit]))
                .values()
                .next()
                .map(|s| s.running())
                .unwrap_or(false);

            let mut stopped = false;
            if stop_row.is_active() && was_running {
                append_log(&format!("Menghentikan layanan {}", service_name));
                match mgr.borrow().stop(&service_id) {
                    Ok((true, _)) => stopped = true,
                    Ok((false, msg)) => {
                        banner.set_title(&format!("Gagal menghentikan layanan: {}", msg));
                        banner.set_revealed(true);
                        busy.set(false);
                        set_running(false);
                        return;
                    }
                    Err(e) => {
                        banner.set_title(&format!("Gagal menghentikan layanan: {}", e));
                        banner.set_revealed(true);
                        busy.set(false);
                        set_running(false);
                        return;
                    }
                }
            }

            {
                let logs = logs.clone();
                let finished = finished.clone();
                let req = req.clone();
                std::thread::spawn(move || {
                    let mut push = |line: String| {
                        logs.lock().unwrap().push(line);
                    };
                    let res = db_restore::restore(&req, &mut push).map_err(|e| e.0);
                    *finished.lock().unwrap() = Some(res);
                });
            }

            {
                let logs = logs.clone();
                let finished = finished.clone();
                let append_log = append_log.clone();
                let set_running = set_running.clone();
                let busy = busy.clone();
                let mgr = mgr.clone();
                let toast = toast.clone();
                let status_label = status_label.clone();
                let service_id = service_id.clone();
                let service_name = service_name.clone();
                adw::glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
                    {
                        let mut queue = logs.lock().unwrap();
                        for line in queue.drain(..) {
                            append_log(&line);
                        }
                    }
                    let done = finished.lock().unwrap().take();
                    if let Some(res) = done {
                        busy.set(false);
                        set_running(false);
                        match res {
                            Ok(()) => {
                                status_label.set_text("Restore selesai");
                                toast.add_toast(adw::Toast::new("Restore database selesai"));
                            }
                            Err(e) => {
                                status_label.set_text(&format!("Gagal: {}", e));
                                append_log(&format!("Gagal: {}", e));
                                let first = e.lines().next().unwrap_or("Restore gagal");
                                toast.add_toast(adw::Toast::new(&format!(
                                    "Restore database gagal: {}",
                                    first
                                )));
                            }
                        }
                        if stopped {
                            append_log(&format!("Menyalakan kembali layanan {}", service_name));
                            if let Err(e) = mgr.borrow().start(&service_id) {
                                append_log(&format!("Gagal menyalakan layanan: {}", e));
                            }
                        }
                        do_refresh();
                        return adw::glib::ControlFlow::Break;
                    }
                    adw::glib::ControlFlow::Continue
                });
            }
        });
    }

    dialog.present(Some(window));

    // Expander log yang terbuka membuat halaman auto-scroll; kembalikan ke atas.
    let page_top = page.clone();
    adw::glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        page_top.scroll_to_top();
    });
}

fn open_cloudflare_dialog(
    window: &adw::ApplicationWindow,
    mgr: &Rc<RefCell<Manager>>,
    toast: &adw::ToastOverlay,
    rebuild_rows: Rc<dyn Fn()>,
) {
    let (config, config_path) = cloudflare::load();
    let cloudflared = Rc::new(RefCell::new(cloudflare::find_cloudflared()));

    let dialog = adw::Dialog::new();
    dialog.set_content_width(620);
    dialog.set_content_height(820);
    dialog.set_title("Cloudflare Tunnel");

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Cloudflare Tunnel",
        "Konfigurasi cloudflared",
    )));
    let cancel_btn = Button::with_label("Batal");
    let dialog_c = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_c.close();
    });
    header.pack_start(&cancel_btn);
    let save_btn = Button::with_label("Simpan");
    save_btn.add_css_class("suggested-action");
    header.pack_end(&save_btn);
    toolbar.add_top_bar(&header);
    dialog.set_child(Some(&toolbar));

    let page = adw::PreferencesPage::new();
    toolbar.set_content(Some(&page));

    // ---- Tunnel ----
    let tunnel_group = adw::PreferencesGroup::new();
    tunnel_group.set_title("Tunnel");
    tunnel_group.set_description(Some(
        "Isi ID/nama tunnel dan file kredensial dari Cloudflare.",
    ));
    let tunnel_row = adw::EntryRow::new();
    tunnel_row.set_title("Tunnel ID / nama");
    tunnel_row.set_text(&config.tunnel);
    let credentials_row = adw::EntryRow::new();
    credentials_row.set_title("Credentials file");
    credentials_row.set_text(&config.credentials_file);
    tunnel_group.add(&tunnel_row);
    tunnel_group.add(&credentials_row);

    let credentials_pick = adw::ActionRow::new();
    credentials_pick.set_title("Pilih file kredensial");
    credentials_pick.set_subtitle("File .json dari Cloudflare (opsional)");
    let credentials_btn = Button::with_label("Pilih File…");
    credentials_btn.set_valign(Align::Center);
    credentials_pick.add_suffix(&credentials_btn);
    credentials_pick.set_activatable_widget(Some(&credentials_btn));
    tunnel_group.add(&credentials_pick);

    let config_row = adw::ActionRow::new();
    config_row.set_title("Lokasi config");
    config_row.set_subtitle(&config_path.to_string_lossy());
    config_row.set_subtitle_selectable(true);
    tunnel_group.add(&config_row);

    let binary_row = adw::ActionRow::new();
    binary_row.set_title("cloudflared");
    match cloudflared.borrow().as_ref() {
        Some(p) => binary_row.set_subtitle(&p.to_string_lossy()),
        None => binary_row.set_subtitle("Belum terpasang. Unduh binary resmi Linux."),
    }
    binary_row.set_subtitle_selectable(true);
    let download_btn = Button::with_label("Unduh");
    download_btn.set_valign(Align::Center);
    download_btn.set_visible(cloudflared.borrow().is_none());
    binary_row.add_suffix(&download_btn);
    tunnel_group.add(&binary_row);
    page.add(&tunnel_group);

    // ---- Ingress ----
    let ingress_group = adw::PreferencesGroup::new();
    ingress_group.set_title("Ingress");
    ingress_group.set_description(Some(
        "Satu per baris: hostname = service. Contoh: app.example.com = http://localhost:8080. Baris tanpa hostname menjadi catch-all.",
    ));
    let ingress_expander = adw::ExpanderRow::new();
    ingress_expander.set_title("Aturan ingress");
    ingress_expander.set_subtitle("hostname = service");
    ingress_expander.set_expanded(true);
    let ingress_scroller = ScrolledWindow::new();
    ingress_scroller.set_min_content_height(160);
    ingress_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    let ingress_view = TextView::new();
    ingress_view.set_monospace(true);
    ingress_view.set_left_margin(8);
    ingress_view.set_right_margin(8);
    ingress_view.set_top_margin(6);
    ingress_view.set_bottom_margin(6);
    ingress_view
        .buffer()
        .set_text(&cloudflare::ingress_text(&config.ingress));
    ingress_scroller.set_child(Some(&ingress_view));
    ingress_expander.add_row(&ingress_scroller);
    ingress_group.add(&ingress_expander);
    page.add(&ingress_group);

    // ---- Opsi ----
    let option_group = adw::PreferencesGroup::new();
    option_group.set_title("Opsi");
    let autoupdate_row = adw::SwitchRow::new();
    autoupdate_row.set_title("Nonaktifkan auto-update");
    autoupdate_row.set_subtitle("Disarankan untuk layanan yang berjalan terus");
    autoupdate_row.set_active(config.no_autoupdate);
    let loglevel_row = adw::ComboRow::new();
    loglevel_row.set_title("Level log");
    let levels = ["info", "debug", "warn", "error"];
    loglevel_row.set_model(Some(&gtk4::StringList::new(&levels)));
    let level_idx = levels
        .iter()
        .position(|&l| l == config.loglevel.as_str())
        .unwrap_or(0);
    loglevel_row.set_selected(level_idx as u32);
    let metrics_row = adw::EntryRow::new();
    metrics_row.set_title("Metrics (opsional)");
    metrics_row.set_text(&config.metrics);
    option_group.add(&autoupdate_row);
    option_group.add(&loglevel_row);
    option_group.add(&metrics_row);
    page.add(&option_group);

    // ---- Daftarkan layanan ----
    let service_group = adw::PreferencesGroup::new();
    service_group.set_title("Layanan Service Center");
    service_group.set_description(Some(
        "Daftarkan cloudflared sebagai layanan systemd user agar otomatis jalan.",
    ));
    let register_row = adw::SwitchRow::new();
    register_row.set_title("Daftarkan ke Service Center");
    register_row.set_active(true);
    let service_name_row = adw::EntryRow::new();
    service_name_row.set_title("Nama layanan");
    service_name_row.set_text("Cloudflare Tunnel");
    let service_autostart_row = adw::SwitchRow::new();
    service_autostart_row.set_title("Mulai otomatis saat boot");
    service_autostart_row.set_active(true);
    let service_start_row = adw::SwitchRow::new();
    service_start_row.set_title("Jalankan sekarang");
    service_start_row.set_active(true);
    service_group.add(&register_row);
    service_group.add(&service_name_row);
    service_group.add(&service_autostart_row);
    service_group.add(&service_start_row);
    page.add(&service_group);

    // ---- Unduh cloudflared ----
    {
        let cloudflared = cloudflared.clone();
        let binary_row = binary_row.clone();
        let download_btn = download_btn.clone();
        let button = download_btn.clone();
        let toast = toast.clone();
        let window = window.clone();
        download_btn.connect_clicked(move |_| {
            button.set_sensitive(false);
            binary_row.set_subtitle("Mengunduh dan memverifikasi binary resmi…");

            let result = std::sync::Arc::new(std::sync::Mutex::new(None));
            let result_worker = result.clone();
            std::thread::spawn(move || {
                *result_worker.lock().unwrap() = Some(cloudflare::install_cloudflared());
            });

            let result_poll = result.clone();
            let cloudflared = cloudflared.clone();
            let binary_row = binary_row.clone();
            let download_btn = button.clone();
            let toast = toast.clone();
            let window = window.clone();
            adw::glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
                let Some(result) = result_poll.lock().unwrap().take() else {
                    return adw::glib::ControlFlow::Continue;
                };
                match result {
                    Ok(path) => {
                        binary_row.set_subtitle(&path.to_string_lossy());
                        download_btn.set_visible(false);
                        *cloudflared.borrow_mut() = Some(path);
                        toast.add_toast(adw::Toast::new("cloudflared berhasil diunduh"));
                    }
                    Err(error) => {
                        binary_row.set_subtitle("Unduhan gagal. Coba lagi atau pasang manual.");
                        download_btn.set_sensitive(true);
                        let alert =
                            adw::AlertDialog::new(Some("Gagal memasang cloudflared"), Some(&error));
                        alert.add_response("ok", "OK");
                        alert.present(Some(&window));
                    }
                }
                adw::glib::ControlFlow::Break
            });
        });
    }

    // ---- Pilih credentials ----
    {
        let credentials_row = credentials_row.clone();
        let window_c = window.clone();
        credentials_btn.connect_clicked(move |_| {
            let credentials_row = credentials_row.clone();
            pick_file_filtered(
                &window_c,
                "Pilih credentials file",
                "JSON (*.json)",
                &["*.json"],
                move |path| {
                    credentials_row.set_text(&path.to_string_lossy());
                },
            );
        });
    }

    // ---- Simpan ----
    {
        let mgr = mgr.clone();
        let toast = toast.clone();
        let rebuild_rows = rebuild_rows.clone();
        let dialog_c = dialog.clone();
        let window_c = window.clone();
        let config_path = config_path.clone();
        let cloudflared = cloudflared.clone();
        save_btn.connect_clicked(move |_| {
            let show_error = |msg: &str| {
                let alert = adw::AlertDialog::new(Some("Tidak bisa melanjutkan"), Some(msg));
                alert.add_response("ok", "OK");
                alert.present(Some(&window_c));
            };
            let tunnel = tunnel_row.text().trim().to_string();
            if tunnel.is_empty() {
                show_error("Tunnel ID / nama wajib diisi.");
                return;
            }
            let credentials_file = credentials_row.text().trim().to_string();
            if !credentials_file.is_empty() && !std::path::Path::new(&credentials_file).is_file() {
                show_error("File kredensial tidak ditemukan.");
                return;
            }
            let buf = ingress_view.buffer();
            let ingress_text = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .to_string();
            let ingress = cloudflare::parse_ingress(&ingress_text);
            let idx = loglevel_row.selected() as usize;
            let loglevel = levels.get(idx).copied().unwrap_or("info").to_string();
            let cfg = cloudflare::CloudflareConfig {
                tunnel,
                credentials_file,
                no_autoupdate: autoupdate_row.is_active(),
                loglevel,
                metrics: metrics_row.text().trim().to_string(),
                ingress,
            };
            let binary = if register_row.is_active() {
                match cloudflared.borrow().clone() {
                    Some(binary) => Some(binary),
                    None => {
                        show_error(
                            "Unduh cloudflared terlebih dahulu sebelum mendaftarkan layanan.",
                        );
                        return;
                    }
                }
            } else {
                None
            };
            if let Err(e) = cloudflare::save(&cfg, &config_path) {
                show_error(&format!("Gagal menulis config: {}", e));
                return;
            }

            if let Some(binary) = binary {
                let name = service_name_row.text().trim().to_string();
                if name.is_empty() {
                    show_error("Nama layanan wajib diisi.");
                    return;
                }
                let command = cloudflare::service_command(&binary, &config_path);
                let workdir = config_path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| {
                        dirs::home_dir()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "/".into())
                    });
                let existing = mgr
                    .borrow()
                    .services()
                    .into_iter()
                    .find(|s| s.file.ends_with("/cloudflared") || s.name == name);
                match existing {
                    Some(svc) => {
                        let result = mgr.borrow_mut().update(
                            &svc.id,
                            &name,
                            &binary.to_string_lossy(),
                            command,
                            &workdir,
                            svc.env.clone(),
                            "on-failure",
                            service_autostart_row.is_active(),
                            false,
                        );
                        match result {
                            Ok((_svc, warnings)) => {
                                if service_start_row.is_active() {
                                    let _ = mgr
                                        .borrow()
                                        .set_enabled(&svc.id, service_autostart_row.is_active());
                                    let _ = mgr.borrow().start(&svc.id);
                                }
                                if !warnings.is_empty() {
                                    toast.add_toast(adw::Toast::new(&warnings.join("; ")));
                                }
                            }
                            Err(e) => {
                                show_error(&e);
                                return;
                            }
                        }
                    }
                    None => {
                        let result = mgr.borrow_mut().add(
                            &name,
                            &binary.to_string_lossy(),
                            command,
                            &workdir,
                            std::collections::BTreeMap::new(),
                            "on-failure",
                            service_autostart_row.is_active(),
                            service_start_row.is_active(),
                        );
                        match result {
                            Ok((_svc, warnings)) => {
                                if !warnings.is_empty() {
                                    toast.add_toast(adw::Toast::new(&warnings.join("; ")));
                                }
                            }
                            Err(e) => {
                                show_error(&e);
                                return;
                            }
                        }
                    }
                }
            }

            dialog_c.close();
            rebuild_rows();
            do_refresh();
            toast.add_toast(adw::Toast::new("Config Cloudflare disimpan"));
        });
    }

    dialog.present(Some(window));
    let page_top = page.clone();
    adw::glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        page_top.scroll_to_top();
    });
}

fn open_add_dialog(
    window: &adw::ApplicationWindow,
    mgr: &Rc<RefCell<Manager>>,
    toast: &adw::ToastOverlay,
    rebuild_rows: Rc<dyn Fn()>,
) {
    let dialog = adw::Dialog::new();
    dialog.set_content_width(560);
    dialog.set_content_height(700);
    dialog.set_title("Tambah Layanan");

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Tambah Layanan", "")));
    let cancel_btn = Button::with_label("Batal");
    let dialog_c = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_c.close();
    });
    header.pack_start(&cancel_btn);
    let save_btn = Button::with_label("Simpan");
    save_btn.add_css_class("suggested-action");
    header.pack_end(&save_btn);
    toolbar.add_top_bar(&header);
    dialog.set_child(Some(&toolbar));

    let page = adw::PreferencesPage::new();
    toolbar.set_content(Some(&page));

    // Aplikasi
    let app_group = adw::PreferencesGroup::new();
    app_group.set_title("Aplikasi");
    app_group.set_description(Some(
        "Pilih file utama program. Perintah dan direktori diisi otomatis.",
    ));
    let file_row = adw::ActionRow::new();
    file_row.set_title("File program");
    file_row.set_subtitle("Belum dipilih");
    file_row.set_subtitle_selectable(true);
    let choose_btn = Button::with_label("Pilih File…");
    choose_btn.set_valign(Align::Center);
    file_row.add_suffix(&choose_btn);
    file_row.set_activatable_widget(Some(&choose_btn));
    app_group.add(&file_row);

    let name_row = adw::EntryRow::new();
    name_row.set_title("Nama layanan");
    let command_row = adw::EntryRow::new();
    command_row.set_title("Perintah");
    let workdir_row = adw::EntryRow::new();
    workdir_row.set_title("Direktori kerja");
    app_group.add(&name_row);
    app_group.add(&command_row);
    app_group.add(&workdir_row);
    page.add(&app_group);

    // Opsi
    let option_group = adw::PreferencesGroup::new();
    option_group.set_title("Opsi");
    let args_row = adw::EntryRow::new();
    args_row.set_title("Argumen tambahan");
    option_group.add(&args_row);
    let restart_row = adw::ComboRow::new();
    restart_row.set_title("Restart otomatis");
    let restart_model = gtk4::StringList::new(&["Saat gagal", "Selalu", "Tidak"]);
    restart_row.set_model(Some(&restart_model));
    restart_row.set_selected(0);
    option_group.add(&restart_row);
    let boot_row = adw::SwitchRow::new();
    boot_row.set_title("Mulai otomatis saat boot");
    boot_row.set_subtitle("Membutuhkan linger agar jalan sebelum login");
    boot_row.set_active(true);
    let start_row = adw::SwitchRow::new();
    start_row.set_title("Jalankan sekarang");
    start_row.set_active(true);
    option_group.add(&boot_row);
    option_group.add(&start_row);
    page.add(&option_group);

    // Environment
    let env_group = adw::PreferencesGroup::new();
    env_group.set_title("Environment");
    let env_expander = adw::ExpanderRow::new();
    env_expander.set_title("Variabel environment");
    env_expander.set_subtitle("Satu per baris: KEY=VALUE");
    let env_scroller = ScrolledWindow::new();
    env_scroller.set_min_content_height(120);
    env_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    let env_view = TextView::new();
    env_view.set_monospace(true);
    env_view.set_left_margin(8);
    env_view.set_right_margin(8);
    env_view.set_top_margin(6);
    env_view.set_bottom_margin(6);
    env_scroller.set_child(Some(&env_view));
    env_expander.add_row(&env_scroller);
    env_group.add(&env_expander);
    page.add(&env_group);

    let selected_file: Rc<RefCell<Option<std::path::PathBuf>>> = Rc::new(RefCell::new(None));

    // Choose file
    {
        let selected = selected_file.clone();
        let file_row_c = file_row.clone();
        let name_row_c = name_row.clone();
        let command_row_c = command_row.clone();
        let workdir_row_c = workdir_row.clone();
        let window_c = window.clone();
        choose_btn.connect_clicked(move |_| {
            let selected = selected.clone();
            let file_row = file_row_c.clone();
            let name_row = name_row_c.clone();
            let command_row = command_row_c.clone();
            let workdir_row = workdir_row_c.clone();
            pick_file(&window_c, "Pilih file program", move |path| {
                file_row.set_subtitle(&path.to_string_lossy());
                if name_row.text().trim().is_empty() {
                    name_row.set_text(&backend::pretty_name(&path));
                }
                match backend::detect_command(&path) {
                    Ok((cmd, wd)) => {
                        command_row.set_text(&shell_join(&cmd));
                        workdir_row.set_text(&wd.to_string_lossy());
                    }
                    Err(_) => {
                        workdir_row.set_text(&path.parent().unwrap_or(&path).to_string_lossy());
                    }
                }
                *selected.borrow_mut() = Some(path);
            });
        });
    }

    // Save
    {
        let mgr = mgr.clone();
        let toast = toast.clone();
        let rebuild_rows = rebuild_rows.clone();
        let selected = selected_file.clone();
        let dialog_c = dialog.clone();
        let window_c = window.clone();
        save_btn.connect_clicked(move |_| {
            let show_error = |msg: &str| {
                let alert = adw::AlertDialog::new(Some("Tidak bisa melanjutkan"), Some(msg));
                alert.add_response("ok", "OK");
                alert.present(Some(&window_c));
            };
            let Some(path) = selected.borrow().clone() else {
                show_error("Pilih file program dulu.");
                return;
            };
            let name = name_row.text().trim().to_string();
            if name.is_empty() {
                show_error("Nama layanan wajib diisi.");
                return;
            }
            let mut command: Vec<String> =
                shell_words::split(command_row.text().as_str()).unwrap_or_default();
            let extra: Vec<String> =
                shell_words::split(args_row.text().as_str()).unwrap_or_default();
            command.extend(extra);
            if command.is_empty() {
                show_error("Perintah masih kosong.");
                return;
            }
            let workdir = if workdir_row.text().trim().is_empty() {
                path.parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                workdir_row.text().to_string()
            };
            if !std::path::Path::new(&workdir).is_dir() {
                show_error("Direktori kerja tidak ditemukan.");
                return;
            }
            let buf = env_view.buffer();
            let text = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .to_string();
            let mut env = std::collections::BTreeMap::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') || !line.contains('=') {
                    continue;
                }
                let mut parts = line.splitn(2, '=');
                let k = parts.next().unwrap_or("").trim().to_string();
                let v = parts.next().unwrap_or("").trim().to_string();
                if !k.is_empty() {
                    env.insert(k, v);
                }
            }
            let idx = restart_row.selected() as usize;
            let restart = backend::RESTART_OPTIONS
                .get(idx)
                .copied()
                .unwrap_or("on-failure");
            let result = mgr.borrow_mut().add(
                &name,
                &path.to_string_lossy(),
                command,
                &workdir,
                env,
                restart,
                boot_row.is_active(),
                start_row.is_active(),
            );
            match result {
                Ok((_svc, warnings)) => {
                    dialog_c.close();
                    rebuild_rows();
                    if warnings.is_empty() {
                        toast.add_toast(adw::Toast::new("Layanan ditambahkan"));
                    } else {
                        toast.add_toast(adw::Toast::new(&warnings.join("; ")));
                    }
                }
                Err(e) => show_error(&e),
            }
        });
    }

    dialog.present(Some(window));
}

fn open_edit_dialog(
    window: &adw::ApplicationWindow,
    mgr: &Rc<RefCell<Manager>>,
    toast: &adw::ToastOverlay,
    service_id: &str,
    rebuild_rows: Rc<dyn Fn()>,
) {
    let service_id = service_id.to_string();
    let svc = match mgr.borrow().get(&service_id) {
        Some(s) => s,
        None => return,
    };

    let dialog = adw::Dialog::new();
    dialog.set_content_width(560);
    dialog.set_content_height(700);
    dialog.set_title("Edit Layanan");

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Edit Layanan", "")));
    let cancel_btn = Button::with_label("Batal");
    let dialog_c = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_c.close();
    });
    header.pack_start(&cancel_btn);
    let save_btn = Button::with_label("Simpan");
    save_btn.add_css_class("suggested-action");
    header.pack_end(&save_btn);
    toolbar.add_top_bar(&header);
    dialog.set_child(Some(&toolbar));

    let page = adw::PreferencesPage::new();
    toolbar.set_content(Some(&page));

    // Aplikasi
    let app_group = adw::PreferencesGroup::new();
    app_group.set_title("Aplikasi");
    app_group.set_description(Some("Ubah file utama, perintah, dan direktori kerja."));
    let file_row = adw::ActionRow::new();
    file_row.set_title("File program");
    file_row.set_subtitle(&svc.file);
    file_row.set_subtitle_selectable(true);
    let choose_btn = Button::with_label("Pilih File…");
    choose_btn.set_valign(Align::Center);
    file_row.add_suffix(&choose_btn);
    file_row.set_activatable_widget(Some(&choose_btn));
    app_group.add(&file_row);

    let name_row = adw::EntryRow::new();
    name_row.set_title("Nama layanan");
    name_row.set_text(&svc.name);
    let command_row = adw::EntryRow::new();
    command_row.set_title("Perintah");
    command_row.set_text(&shell_join(&svc.command));
    let workdir_row = adw::EntryRow::new();
    workdir_row.set_title("Direktori kerja");
    workdir_row.set_text(&svc.workdir);
    app_group.add(&name_row);
    app_group.add(&command_row);
    app_group.add(&workdir_row);
    page.add(&app_group);

    // Opsi
    let option_group = adw::PreferencesGroup::new();
    option_group.set_title("Opsi");
    let args_row = adw::EntryRow::new();
    args_row.set_title("Argumen tambahan");
    option_group.add(&args_row);
    let restart_row = adw::ComboRow::new();
    restart_row.set_title("Restart otomatis");
    let restart_model = gtk4::StringList::new(&["Saat gagal", "Selalu", "Tidak"]);
    restart_row.set_model(Some(&restart_model));
    let restart_idx = backend::RESTART_OPTIONS
        .iter()
        .position(|&r| r == svc.restart)
        .unwrap_or(0);
    restart_row.set_selected(restart_idx as u32);
    option_group.add(&restart_row);
    let boot_row = adw::SwitchRow::new();
    boot_row.set_title("Mulai otomatis saat boot");
    boot_row.set_subtitle("Membutuhkan linger agar jalan sebelum login");
    let enabled = mgr
        .borrow()
        .states(Some(vec![svc.unit_name()]))
        .get(&svc.unit_name())
        .map(|s| s.enabled == "enabled")
        .unwrap_or(false);
    boot_row.set_active(enabled);
    let restart_now_row = adw::SwitchRow::new();
    restart_now_row.set_title("Restart layanan setelah simpan");
    let running = mgr
        .borrow()
        .states(Some(vec![svc.unit_name()]))
        .get(&svc.unit_name())
        .map(|s| s.running())
        .unwrap_or(false);
    restart_now_row.set_active(running);
    option_group.add(&boot_row);
    option_group.add(&restart_now_row);
    page.add(&option_group);

    // Environment
    let env_group = adw::PreferencesGroup::new();
    env_group.set_title("Environment");
    let env_expander = adw::ExpanderRow::new();
    env_expander.set_title("Variabel environment");
    env_expander.set_subtitle("Satu per baris: KEY=VALUE");
    let env_scroller = ScrolledWindow::new();
    env_scroller.set_min_content_height(120);
    env_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    let env_view = TextView::new();
    env_view.set_monospace(true);
    env_view.set_left_margin(8);
    env_view.set_right_margin(8);
    env_view.set_top_margin(6);
    env_view.set_bottom_margin(6);
    env_scroller.set_child(Some(&env_view));
    env_expander.add_row(&env_scroller);
    env_group.add(&env_expander);
    page.add(&env_group);

    let env_text = svc
        .env
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n");
    env_view.buffer().set_text(&env_text);

    let selected_file: Rc<RefCell<Option<std::path::PathBuf>>> =
        Rc::new(RefCell::new(Some(std::path::PathBuf::from(&svc.file))));

    // Choose file
    {
        let selected = selected_file.clone();
        let file_row_c = file_row.clone();
        let name_row_c = name_row.clone();
        let command_row_c = command_row.clone();
        let workdir_row_c = workdir_row.clone();
        let window_c = window.clone();
        choose_btn.connect_clicked(move |_| {
            let selected = selected.clone();
            let file_row = file_row_c.clone();
            let name_row = name_row_c.clone();
            let command_row = command_row_c.clone();
            let workdir_row = workdir_row_c.clone();
            pick_file(&window_c, "Pilih file program", move |path| {
                file_row.set_subtitle(&path.to_string_lossy());
                if name_row.text().trim().is_empty() {
                    name_row.set_text(&backend::pretty_name(&path));
                }
                match backend::detect_command(&path) {
                    Ok((cmd, wd)) => {
                        command_row.set_text(&shell_join(&cmd));
                        workdir_row.set_text(&wd.to_string_lossy());
                    }
                    Err(_) => {
                        workdir_row.set_text(&path.parent().unwrap_or(&path).to_string_lossy());
                    }
                }
                *selected.borrow_mut() = Some(path);
            });
        });
    }

    // Save
    {
        let mgr = mgr.clone();
        let toast = toast.clone();
        let rebuild_rows = rebuild_rows.clone();
        let selected = selected_file.clone();
        let dialog_c = dialog.clone();
        let window_c = window.clone();
        let sid = service_id.clone();
        save_btn.connect_clicked(move |_| {
            let show_error = |msg: &str| {
                let alert = adw::AlertDialog::new(Some("Tidak bisa melanjutkan"), Some(msg));
                alert.add_response("ok", "OK");
                alert.present(Some(&window_c));
            };
            let Some(path) = selected.borrow().clone() else {
                show_error("Pilih file program dulu.");
                return;
            };
            let name = name_row.text().trim().to_string();
            if name.is_empty() {
                show_error("Nama layanan wajib diisi.");
                return;
            }
            let mut command: Vec<String> =
                shell_words::split(command_row.text().as_str()).unwrap_or_default();
            let extra: Vec<String> =
                shell_words::split(args_row.text().as_str()).unwrap_or_default();
            command.extend(extra);
            if command.is_empty() {
                show_error("Perintah masih kosong.");
                return;
            }
            let workdir = if workdir_row.text().trim().is_empty() {
                path.parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                workdir_row.text().to_string()
            };
            if !std::path::Path::new(&workdir).is_dir() {
                show_error("Direktori kerja tidak ditemukan.");
                return;
            }
            let buf = env_view.buffer();
            let text = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .to_string();
            let mut env = std::collections::BTreeMap::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') || !line.contains('=') {
                    continue;
                }
                let mut parts = line.splitn(2, '=');
                let k = parts.next().unwrap_or("").trim().to_string();
                let v = parts.next().unwrap_or("").trim().to_string();
                if !k.is_empty() {
                    env.insert(k, v);
                }
            }
            let idx = restart_row.selected() as usize;
            let restart = backend::RESTART_OPTIONS
                .get(idx)
                .copied()
                .unwrap_or("on-failure");
            let result = mgr.borrow_mut().update(
                &sid,
                &name,
                &path.to_string_lossy(),
                command,
                &workdir,
                env,
                restart,
                boot_row.is_active(),
                restart_now_row.is_active(),
            );
            match result {
                Ok((_svc, warnings)) => {
                    dialog_c.close();
                    rebuild_rows();
                    do_refresh();
                    if warnings.is_empty() {
                        toast.add_toast(adw::Toast::new("Layanan diperbarui"));
                    } else {
                        toast.add_toast(adw::Toast::new(&warnings.join("; ")));
                    }
                }
                Err(e) => show_error(&e),
            }
        });
    }

    dialog.present(Some(window));

    let page_top = page.clone();
    adw::glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        page_top.scroll_to_top();
    });
}

fn pick_file<W, F>(window: &W, title: &str, cb: F)
where
    W: IsA<gtk4::Window>,
    F: Fn(std::path::PathBuf) + 'static,
{
    let dialog = gtk4::FileChooserDialog::new(
        Some(title),
        Some(window),
        gtk4::FileChooserAction::Open,
        &[
            ("Batal", gtk4::ResponseType::Cancel),
            ("Buka", gtk4::ResponseType::Accept),
        ],
    );
    dialog.set_default_size(1000, 700);
    dialog.set_modal(true);
    dialog.connect_response(move |d, resp| {
        if resp == gtk4::ResponseType::Accept {
            if let Some(file) = d.file() {
                if let Some(path) = file.path() {
                    cb(path);
                }
            }
        }
        d.close();
    });
    dialog.present();
}

fn pick_file_filtered<W, F>(window: &W, title: &str, filter_name: &str, patterns: &[&str], cb: F)
where
    W: IsA<gtk4::Window>,
    F: Fn(std::path::PathBuf) + 'static,
{
    let dialog = gtk4::FileChooserDialog::new(
        Some(title),
        Some(window),
        gtk4::FileChooserAction::Open,
        &[
            ("Batal", gtk4::ResponseType::Cancel),
            ("Buka", gtk4::ResponseType::Accept),
        ],
    );
    dialog.set_default_size(1000, 700);
    dialog.set_modal(true);
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some(filter_name));
    for pattern in patterns {
        filter.add_pattern(pattern);
    }
    dialog.set_filter(&filter);
    let all = gtk4::FileFilter::new();
    all.set_name(Some("Semua file"));
    all.add_pattern("*");
    dialog.add_filter(&all);
    dialog.connect_response(move |d, resp| {
        if resp == gtk4::ResponseType::Accept {
            if let Some(file) = d.file() {
                if let Some(path) = file.path() {
                    cb(path);
                }
            }
        }
        d.close();
    });
    dialog.present();
}
