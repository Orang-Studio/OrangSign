mod ops;
mod ui;
use gpui::{App, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions, prelude::*, px, size,};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut file = None;
    let mut auto_sign = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--elevated" => {
                let action = args.next().and_then(|a| ops::Action::parse(&a));
                let mut cert = None;
                while let Some(a) = args.next() {
                    if a == "--cert" {cert = args.next().map(PathBuf::from);}
                }
                let Some(action) = action else {
                    eprintln!("unknown elevated action");
                    std::process::exit(2);
                };
                std::process::exit(ops::elevated_main(action, cert));
            }
            "--uninstall" => {
                let mut log = Vec::new();
                let code = match ops::uninstall(&mut log) {
                    Ok(()) => 0,
                    Err(e) => {
                        log.push(format!("Error: {e:#}"));
                        1
                    }
                };
                for line in log {
                    eprintln!("{line}");
                }
                std::process::exit(code);
            }
            "--sign" => {
                file = args.next().map(PathBuf::from);
                auto_sign = true;
            }
            other if !other.starts_with("--") && file.is_none() => {
                file = Some(PathBuf::from(other));
            }
            _ => {}
        }
    }

    ops::ensure_user_icon();

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(580.)), cx);
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("OrangSign".into()),
                ..Default::default()
            }),
            window_background: WindowBackgroundAppearance::Transparent,
            window_min_size: Some(size(px(760.), px(460.))),
            app_id: Some(ops::APP_ID.into()),
            ..Default::default()
        };

        cx.open_window(opts, |window, cx| {
            window.set_window_title("OrangSign");
            cx.new(|cx| ui::OrangSign::new(file, auto_sign, cx))
        })
        .expect("failed to open window");

        ui::quit_on_close(cx);
        cx.activate(true);
    });
}



