use super::workspace;
use crate::infrastructure::persistence::workspace_store as storage;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

pub(super) fn run() {
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
        if let Err(e) = crate::infrastructure::omp::probe() {
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
                workspace::AcceptCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "enter",
                workspace::AcceptCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "down",
                workspace::NextCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "up",
                workspace::PreviousCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "escape",
                workspace::DismissCompletion,
                Some("Workspace > ComposerCompletion > Input"),
            ),
            KeyBinding::new(
                "escape",
                workspace::DismissCompletion,
                Some("Workspace > ComposerHint > Input"),
            ),
            KeyBinding::new(
                "enter",
                workspace::NextSearchMatch,
                Some("Workspace > SessionSearch > Input"),
            ),
            KeyBinding::new(
                "shift-enter",
                workspace::PreviousSearchMatch,
                Some("Workspace > SessionSearch > Input"),
            ),
            KeyBinding::new(
                "escape",
                workspace::CloseSearch,
                Some("Workspace > SessionSearch"),
            ),
            KeyBinding::new("ctrl-f", workspace::FindInSession, Some("Workspace")),
            KeyBinding::new(
                "ctrl-f",
                workspace::FindInSession,
                Some("Workspace > Input"),
            ),
            KeyBinding::new("ctrl-enter", workspace::SendPrompt, Some("Workspace")),
            KeyBinding::new(
                "ctrl-enter",
                workspace::SendPrompt,
                Some("Workspace > Input"),
            ),
            KeyBinding::new("ctrl-alt-m", workspace::AddComment, Some("Workspace")),
            KeyBinding::new("ctrl-shift-b", workspace::TogglePanel, Some("Workspace")),
            KeyBinding::new("ctrl-n", workspace::NewSession, Some("Workspace")),
            KeyBinding::new("ctrl-o", workspace::OpenProject, Some("Workspace")),
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
        super::theme::apply_accent(cx);
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
            cx.new(|cx| workspace::Workspace::new(window, cx, demo, project.clone()))
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
