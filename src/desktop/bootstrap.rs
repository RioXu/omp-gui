use super::workspace;
use crate::infrastructure::persistence::workspace_store as storage;
use gpui_kit::component::{ActiveTheme, Theme, TitleBar};
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
    #[cfg(feature = "frame-profile")]
    let frame_profile_hz = demo
        .then(|| option("--frame-profile-hz").and_then(|hz| hz.parse::<f64>().ok()))
        .flatten();
    application().with_assets(assets::AllAssets).run(move |cx| {
        cx.set_app_identity("RioXu.OMP.GUI", "OMP GUI");
        init(cx);
        cx.bind_keys([
            KeyBinding::new(
                "up",
                workspace::PreviousInput,
                Some("Workspace > ComposerHistory > Input"),
            ),
            KeyBinding::new(
                "down",
                workspace::NextInput,
                Some("Workspace > ComposerHistory > Input"),
            ),
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
            KeyBinding::new("ctrl-s", workspace::SaveDocument, Some("Workspace")),
            KeyBinding::new("ctrl-s", workspace::SaveDocument, Some("Workspace > Input")),
        ]);
        Theme::update(cx, |t| {
            t.font_family = "Microsoft YaHei UI".into();
            t.font_size = px(14.);
        });
        let saved = storage::load().map(|db| db.theme).unwrap_or_default();
        let skin = super::theme::Skin::from_saved(&saved, cx.theme().is_dark());
        super::theme::apply(skin, None, cx);
        let display_id = None;
        #[cfg(feature = "frame-profile")]
        let display_id = frame_profile_hz
            .and_then(|hz| {
                cx.displays()
                    .into_iter()
                    .find(|display| {
                        display
                            .refresh_interval()
                            .is_some_and(|interval| (1. / interval.as_secs_f64() - hz).abs() < 1.)
                    })
                    .map(|display| display.id())
            })
            .or(display_id);
        let bounds = Bounds::centered(display_id, size(px(1240.), px(850.)), cx);
        let options = WindowOptions {
            display_id,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(860.), px(640.))),
            titlebar: Some(TitlebarOptions {
                title: Some(if demo { "OMP · 演示" } else { "OMP" }.into()),
                ..TitleBar::title_bar_options()
            }),
            ..TitleBar::window_options()
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
