#![windows_subsystem = "windows"]
mod app;
mod domain;
mod engine;
mod file_tree;
mod files;
mod registry;
mod registry_ui;
mod settings;
mod settings_ui;
mod storage;
mod ui;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let demo = args.iter().any(|a| a == "--demo");
    if let Some(dir) = option("--data-dir") {
        unsafe {
            std::env::set_var("OMP_GUI_DATA_DIR", dir);
        }
    } else if demo {
        let dir = storage::data_dir().join("demo");
        unsafe {
            std::env::set_var("OMP_GUI_DATA_DIR", dir);
        }
    }
    if args.iter().any(|a| a == "--probe") {
        if let Err(e) = probe() {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
        return;
    }
    let project = option("--project").map(std::path::PathBuf::from);
    application().with_assets(assets::AllAssets).run(move |cx| {
        init(cx);
        cx.bind_keys([
            KeyBinding::new(
                "tab",
                app::AcceptCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "enter",
                app::AcceptCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "down",
                app::NextCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "up",
                app::PreviousCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "escape",
                app::DismissCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "escape",
                app::DismissCompletion,
                Some("Workspace > ComposerHint > Input"),
            ),
            KeyBinding::new(
                "enter",
                app::NextSearchMatch,
                Some("Workspace > SessionSearch > Input"),
            ),
            KeyBinding::new(
                "shift-enter",
                app::PreviousSearchMatch,
                Some("Workspace > SessionSearch > Input"),
            ),
            KeyBinding::new(
                "escape",
                app::CloseSearch,
                Some("Workspace > SessionSearch"),
            ),
            KeyBinding::new("ctrl-f", app::FindInSession, Some("Workspace")),
            KeyBinding::new("ctrl-f", app::FindInSession, Some("Workspace > Input")),
            KeyBinding::new("ctrl-enter", app::SendPrompt, Some("Workspace")),
            KeyBinding::new("ctrl-enter", app::SendPrompt, Some("Workspace > Input")),
            KeyBinding::new("ctrl-alt-m", app::AddComment, Some("Workspace")),
            KeyBinding::new("ctrl-shift-b", app::TogglePanel, Some("Workspace")),
            KeyBinding::new("ctrl-n", app::NewSession, Some("Workspace")),
            KeyBinding::new("ctrl-o", app::OpenProject, Some("Workspace")),
        ]);
        Theme::update(cx, |t| {
            t.font_family = "Microsoft YaHei UI".into();
            t.font_size = px(14.);
        });
        if let Ok(db) = storage::load() {
            match db.theme.as_str() {
                "dark" => Theme::change(ThemeMode::Dark, None, cx),
                "light" => Theme::change(ThemeMode::Light, None, cx),
                _ => {}
            }
        }
        app::apply_accent(cx);
        let bounds = Bounds::centered(None, size(px(1240.), px(850.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(860.), px(640.))),
            titlebar: Some(TitlebarOptions {
                title: Some(if demo { "OMP · 演示" } else { "OMP" }.into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        open_window(options, cx, move |window, cx| {
            cx.new(|cx| app::Workspace::new(window, cx, demo, project.clone()))
        })
        .expect("Unable to open OMP window");
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}

fn probe() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    let root = std::env::current_dir()?;
    let runtime = engine::Engine::start(
        "omp",
        &root,
        &storage::data_dir().join("probe-sessions"),
        None,
    )?;
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        match runtime.events.recv_timeout(Duration::from_secs(1)) {
            Ok(engine::Event::Frame(v)) if v["type"] == "ready" => {
                runtime.send(serde_json::json!({"type":"negotiate_protocol","id":"probe-protocol","protocolVersion":2}))?;
                runtime.send(serde_json::json!({"type":"get_state","id":"probe-state"}))?;
            }
            Ok(engine::Event::Frame(v)) if v["id"] == "probe-state" => {
                anyhow::ensure!(v["success"] == true, "OMP state request failed");
                println!(
                    "OMP handshake OK; model={}; no prompt sent",
                    v["data"]["model"]["id"].as_str().unwrap_or("unconfigured")
                );
                return Ok(());
            }
            Ok(engine::Event::Error(e)) => eprintln!("{e}"),
            Ok(engine::Event::Closed) => anyhow::bail!("OMP exited before handshake"),
            _ => {}
        }
    }
    anyhow::bail!("OMP handshake timed out")
}
