use crate::ops::{self, Action, Log, SystemState};
use gpui::{App, ClickEvent, Context, Div, ExternalPaths, FontWeight, PathPromptOptions, ScrollHandle, SharedString, Stateful, Window, div, prelude::*, px, rgb, rgba,};
use std::path::PathBuf;

const BG_RGBA: u32 = 0x000000e6;
const PANEL_HI: u32 = 0x161616;
const BORDER: u32 = 0x2a2a2a;
const TEXT: u32 = 0xffffff;
const MUTED: u32 = 0x8e8e8e;
const WHITE: u32 = 0xffffff;
const WHITE_HOVER: u32 = 0xdcdcdc;
const BLACK: u32 = 0x000000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Sign = 0,
    Certificate = 1,
    System = 2,
}

#[derive(Clone)]
enum Status {Idle, Ok(SharedString), Err(SharedString),}

pub struct OrangSign {
    tab: Tab,
    file: Option<PathBuf>,
    logs: [Log; 3],
    scroll: [ScrollHandle; 3],
    status: Status,
    busy: bool,
    system: SystemState,
}

impl OrangSign {
    pub fn new(file: Option<PathBuf>, auto_sign: bool, cx: &mut Context<Self>) -> Self {
        let mut app = Self {
            tab: Tab::Sign,
            file,
            logs: Default::default(),
            scroll: Default::default(),
            status: Status::Idle,
            busy: false,
            system: SystemState::default(),
        };
        app.refresh_system(cx);
        if auto_sign && app.file.is_some() {
            app.sign(cx);
        }
        app
    }

    fn push_log(&mut self, tab: Tab, line: impl Into<String>) {
        self.logs[tab as usize].push(line.into());
        self.scroll[tab as usize].scroll_to_bottom();
    }

    fn run_job(
        &mut self,
        tab: Tab,
        success: &'static str,
        job: impl FnOnce(&mut Log) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = Status::Idle;
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            let mut log = Log::new();
            let res = job(&mut log);
            (log, res)
        });

        cx.spawn(async move |this, cx| {
            let (log, res) = task.await;
            let _ = this.update(cx, |this, cx| {
                for line in log {
                    this.push_log(tab, line);
                }
                match res {
                    Ok(()) => this.status = Status::Ok(success.into()),
                    Err(e) => {
                        this.push_log(tab, format!("Error: {e:#}"));
                        this.status = Status::Err(format!("{e}").into());
                    }
                }
                this.busy = false;
                this.refresh_system(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_system(&mut self, cx: &mut Context<Self>) {
        let task = cx
            .background_executor()
            .spawn(async { ops::system_state() });
        cx.spawn(async move |this, cx| {
            let state = task.await;
            let _ = this.update(cx, |this, cx| {
                this.system = state;
                cx.notify();
            });
        })
        .detach();
    }

    fn browse(&mut self, directories: bool, cx: &mut Context<Self>) {
        let prompt = if directories {"Select folder to sign"} else {"Select file to sign"};
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: Some(prompt.into()),
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await {
                if let Some(path) = paths.pop() {
                    let _ = this.update(cx, |this, cx| {
                        this.file = Some(path);
                        this.status = Status::Idle;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn sign(&mut self, cx: &mut Context<Self>) {
        self.tab = Tab::Sign;
        let Some(file) = self.file.clone() else {
            self.status = Status::Err("Please select file or folder to sign.".into());
            cx.notify();
            return;
        };
        if !ops::cert_path().is_file() {
            self.status =
                Status::Err("Certificate not found. Please create a certificate first.".into());
            cx.notify();
            return;
        }

        let msg = if file.is_dir() {
            "Folder signed successfully!"
        } else {
            "File signed successfully!"
        };
        self.run_job(Tab::Sign, msg, move |log| ops::sign_file(&file, log), cx);
    }

    fn button(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        primary: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        self.button_enabled(id, label, primary, true, cx, on_click)
    }

    fn button_enabled(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        primary: bool,
        enabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let disabled = self.busy || !enabled;
        let (bg, hover, fg, border) = if primary {
            (rgb(WHITE), rgb(WHITE_HOVER), rgb(BLACK), rgb(WHITE))
        } else {
            (rgb(BLACK), rgb(PANEL_HI), rgb(TEXT), rgb(BORDER))
        };

        let btn = div()
            .id(id)
            .px_4()
            .py_1p5()
            .rounded_md()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .bg(bg)
            .text_color(fg)
            .border_1()
            .border_color(border)
            .child(label.into());

        if disabled {
            btn.opacity(0.4)
        } else {
            btn.cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
        }
    }

    fn section(title: &'static str) -> Div {
        div().flex().flex_col().gap_2().py_1().child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(MUTED))
                .child(title.to_uppercase()),
        )
    }

    fn log_view(&self, tab: Tab) -> impl IntoElement {
        let lines = &self.logs[tab as usize];
        Self::section("Status")
            .flex_1()
            .min_h(px(80.))
            .border_t_1()
            .border_color(rgb(BORDER))
            .pt_3()
            .child(
                div()
                    .id(("log", tab as usize))
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll[tab as usize])
                    .font_family("monospace")
                    .text_xs()
                    .text_color(rgb(TEXT))
                    .children(lines.iter().map(|line| {
                        let color = if line.starts_with("Error") || line.contains("successfully") {
                            rgb(WHITE)
                        } else {
                            rgb(MUTED)
                        };
                        div()
                            .text_color(color)
                            .child(SharedString::from(line.clone()))
                    })),
            )
    }

    fn badge(label: &'static str, active: Option<bool>) -> Div {
        let (color, border) = if active == Some(true) {
            (rgb(WHITE), rgb(WHITE))
        } else {
            (rgb(MUTED), rgb(BORDER))
        };
        div()
            .px_2()
            .py_0p5()
            .rounded_md()
            .text_xs()
            .border_1()
            .border_color(border)
            .text_color(color)
            .child(label)
    }

    fn render_sign_tab(&mut self, cx: &mut Context<Self>) -> Div {
        let cert = ops::cert_path();
        let cert_ok = cert.is_file();
        let file_label: SharedString = match &self.file {
            Some(p) => p.display().to_string().into(),
            None => "Drop file or folder here, or click Browse".into(),
        };
        let has_file = self.file.is_some();

        let hint = if self.system.has_osslsigncode {
            "Windows binaries get Authenticode signature via osslsigncode; all other files get detached PKCS#7 signature via OpenSSL."
        } else {
            "Creates detached signature via OpenSSL. Selecting a folder signs all files inside it"
        };

        div()
            .flex()
            .flex_col()
            .gap_4()
            .flex_1()
            .child(
                Self::section("Certificate")
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(rgb(TEXT))
                                    .child(SharedString::from(cert.display().to_string())),
                            )
                            .child(if cert_ok {
                                Self::badge("found", Some(true))
                            } else {
                                Self::badge("missing", None)
                            }),
                    )
                    .when_some(self.system.cert_info.clone(), |d, info| {
                        d.child(
                            div()
                                .font_family("monospace")
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(SharedString::from(info)),
                        )
                    }),
            )
            .child(div().h(px(1.)).bg(rgb(BORDER)))
            .child(
                Self::section("File or folder to sign")
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .id("drop-zone")
                                    .flex_1()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_dashed()
                                    .border_color(rgb(BORDER))
                                    .text_sm()
                                    .text_color(if has_file { rgb(TEXT) } else { rgb(MUTED) })
                                    .drag_over::<ExternalPaths>(|s, _, _, _| {
                                        s.border_color(rgb(WHITE)).bg(rgb(PANEL_HI))
                                    })
                                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                                        if let Some(p) = paths.paths().first() {
                                            this.file = Some(p.clone());
                                            this.status = Status::Idle;
                                            cx.notify();
                                        }
                                    }))
                                    .child(file_label),
                            )
                            .child(self.button(
                                "browse-file",
                                "Browse File..",
                                false,
                                cx,
                                |this, cx| this.browse(false, cx),
                            ))
                            .child(self.button(
                                "browse-folder",
                                "Select Folder..",
                                false,
                                cx,
                                |this, cx| this.browse(true, cx),
                            ))
                            .child(self.button("sign", "Sign", true, cx, |this, cx| this.sign(cx)))
                            .child(self.button("close", "Close", false, cx, |_, cx| cx.quit())),
                    )
                    .child(div().text_xs().text_color(rgb(MUTED)).child(hint)),
            )
            .child(self.log_view(Tab::Sign))
    }

    fn render_cert_tab(&mut self, cx: &mut Context<Self>) -> Div {
        let exists = ops::cert_path().is_file();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .flex_1()
            .child(
                Self::section("Create certificate")
                    .child(div().text_sm().text_color(rgb(TEXT)).child(format!(
                        "This will create new certificate ({}) using OpenSSL \
                             (RSA 2048, SHA-256, valid for 5 years) and export it as PKCS#12.",
                        ops::CERT_SUBJECT
                    )))
                    .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                        "Saved to: {} (mode 0600)",
                        ops::cert_path().display()
                    )))
                    .when(exists, |d| {
                        d.child(div().text_xs().text_color(rgb(WHITE)).child(
                            "certificate already exists - creating a new one replaces it \
                             (re-run \"Make trusted root\" on System tab afterwards).",
                        ))
                    })
                    .child(div().flex().child(self.button(
                        "create-cert",
                        "Create Certificate",
                        true,
                        cx,
                        |this, cx| {
                            this.run_job(
                                Tab::Certificate,
                                "Certificate created successfully!",
                                ops::create_certificate,
                                cx,
                            )
                        },
                    ))),
            )
            .child(self.log_view(Tab::Certificate))
    }

    fn system_row(
        &self,
        title: &'static str,
        desc: String,
        state: Option<bool>,
        on_label: &'static str,
        off_label: &'static str,
        button: Stateful<Div>,
    ) -> Div {
        let badge_text = if state == Some(true) {
            on_label
        } else {
            off_label
        };
        div()
            .flex()
            .items_center()
            .gap_3()
            .py_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(title),
                            )
                            .child(Self::badge(badge_text, state)),
                    )
                    .child(div().text_xs().text_color(rgb(MUTED)).child(desc)),
            )
            .child(button)
    }

    fn render_system_tab(&mut self, cx: &mut Context<Self>) -> Div {
        let cert_exists = ops::cert_path().is_file();
        let installed = self.system.installed;
        let context_menu = self.system.context_menu;
        let trusted = self.system.trusted;

        let install_btn = if installed {
            self.button("uninstall", "Uninstall", false, cx, |this, cx| {
                this.run_job(Tab::System, "OrangSign uninstalled.", ops::uninstall, cx)
            })
        } else {
            self.button("install", "Install", true, cx, |this, cx| {
                this.run_job(Tab::System, "OrangSign installed.", ops::install, cx)
            })
        };

        let menu_btn = if context_menu {
            self.button("menu-remove", "Remove", false, cx, |this, cx| {
                this.run_job(
                    Tab::System,
                    "Context menu entry removed.",
                    ops::remove_context_menu,
                    cx,
                )
            })
        } else {
            self.button_enabled(
                "menu-add",
                "Add to context menu",
                true,
                installed,
                cx,
                |this, cx| {
                    this.run_job(
                        Tab::System,
                        "Context menu entry added.",
                        ops::add_context_menu,
                        cx,
                    )
                },
            )
        };

        let trust_btn = if trusted == Some(true) {
            self.button("untrust", "Remove trust", false, cx, |this, cx| {
                this.run_job(
                    Tab::System,
                    "Certificate is no longer trusted.",
                    |log| ops::run_elevated(Action::Untrust, log),
                    cx,
                )
            })
        } else {
            self.button_enabled(
                "trust",
                "Make trusted root",
                true,
                cert_exists,
                cx,
                |this, cx| {
                    this.run_job(
                        Tab::System,
                        "Certificate is now a trusted root.",
                        |log| ops::run_elevated(Action::Trust, log),
                        cx,
                    )
                },
            )
        };

        let menu_desc = if installed {
            format!(
                "Adds \"{}\" to the right-click menu in Dolphin, Nemo, and Nautilus.",
                ops::CONTEXT_MENU_TEXT
            )
        } else {
            "Install OrangSign into system first to enable adding it to the context menu.".into()
        };

        let trust_desc = if cert_exists {
            "Installs public certificate into system CA trust anchors.".into()
        } else {
            "Create a certificate first.".into()
        };

        div()
            .flex()
            .flex_col()
            .gap_4()
            .flex_1()
            .child(
                Self::section("System integration")
                    .child(self.system_row(
                        "Install into system",
                        format!(
                            "Copies binary to {} and installs desktop launcher + icon.",
                            ops::installed_exe().display()
                        ),
                        Some(installed),
                        "installed",
                        "not installed",
                        install_btn,
                    ))
                    .child(div().h(px(1.)).bg(rgb(BORDER)))
                    .child(self.system_row(
                        "File manager context menu",
                        menu_desc,
                        Some(context_menu),
                        "added",
                        if installed {
                            "not added"
                        } else {
                            "requires install"
                        },
                        menu_btn,
                    ))
                    .child(div().h(px(1.)).bg(rgb(BORDER)))
                    .child(self.system_row(
                        "Trusted root certificate",
                        trust_desc,
                        trusted,
                        "trusted",
                        if cert_exists {
                            "not trusted"
                        } else {
                            "no certificate"
                        },
                        trust_btn,
                    )),
            )
            .child(self.log_view(Tab::System))
    }

    fn tab_button(&self, tab: Tab, label: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        let active = self.tab == tab;
        div()
            .id(("tab", tab as usize))
            .px_4()
            .py_2()
            .text_sm()
            .cursor_pointer()
            .border_b_2()
            .border_color(if active { rgb(WHITE) } else { rgba(0x00000000) })
            .text_color(if active { rgb(TEXT) } else { rgb(MUTED) })
            .when(active, |d| d.font_weight(FontWeight::SEMIBOLD))
            .hover(|s| s.text_color(rgb(TEXT)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
            .child(label)
    }
}

impl Render for OrangSign {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.tab {
            Tab::Sign => self.render_sign_tab(cx),
            Tab::Certificate => self.render_cert_tab(cx),
            Tab::System => self.render_system_tab(cx),
        };

        let status = if self.busy {
            Some(div().text_xs().text_color(rgb(TEXT)).child("Working..."))
        } else {
            match &self.status {
                Status::Ok(m) => Some(div().text_xs().text_color(rgb(TEXT)).child(m.clone())),
                Status::Err(m) => Some(div().text_xs().text_color(rgb(MUTED)).child(m.clone())),
                Status::Idle => None,
            }
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba(BG_RGBA))
            .text_color(rgb(TEXT))
            .p_4()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(self.tab_button(Tab::Sign, "Sign File", cx))
                    .child(self.tab_button(Tab::Certificate, "Create Certificate", cx))
                    .child(self.tab_button(Tab::System, "System", cx))
                    .child(div().flex_1())
                    .when_some(status, |d, s| d.child(s.px_2())),
            )
            .child(body)
    }
}

pub fn quit_on_close(cx: &mut App) {
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}