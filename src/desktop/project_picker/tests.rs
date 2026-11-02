use super::{directory_item, path_label};
use core::prelude::v1::test;
use gpui_kit::{
    AppContext, Bounds, Context, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    WindowBounds, WindowOptions, div, open_window, point, prelude::*, px, size,
};
use omp_core::{environment::HostPath, host::DirectoryEntry};

struct Row;
impl Render for Row {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(directory_item(
            &DirectoryEntry {
                name: "目录 空格".into(),
                path: HostPath::Local("directory".into()),
            },
            0,
            false,
        ))
    }
}
#[gpui_kit::test]
fn directory_icon_and_name_share_one_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, _) = cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(500.), px(200.)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(|_| Row),
        )
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let icon = visual.debug_bounds("project-directory-icon").unwrap();
    let name = visual.debug_bounds("project-directory-name").unwrap();
    assert!(name.left() >= icon.right());
    assert!(
        name.center().y - icon.center().y < px(1.) && name.center().y - icon.center().y > px(-1.),
        "icon and filename must be horizontally aligned"
    );
    assert!(name.bottom() <= px(32.) && icon.bottom() <= px(32.));
}
#[test]
fn recent_directory_labels_hide_windows_transport_prefixes() {
    assert_eq!(
        path_label(&HostPath::Local(r"\\?\E:\dev\demo".into())),
        r"E:\dev\demo"
    );
    assert_eq!(
        path_label(&HostPath::Posix("/home/rio/a\\b".into())),
        "/home/rio/a\\b"
    );
}

#[gpui_kit::test]
fn directory_feedback_does_not_move_list_or_dialog_footer(cx: &mut TestAppContext) {
    use super::ProjectPicker;
    use gpui_kit::component::{
        dialog::{DialogClose, DialogFooter},
        input::InputState,
    };
    use omp_core::host::{Directory, LocalHost};
    use std::sync::{Arc, mpsc};
    cx.update(gpui_kit::init);
    let (window, _) =
        cx.update(|cx| open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Row)).unwrap());
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1000.), px(800.)));
    let picker = visual.update(|window, cx| {
        cx.set_reduce_motion(true);
        cx.new(|cx| {
            let (tx, rx) = mpsc::channel();
            ProjectPicker {
                browser: Arc::new(LocalHost),
                path: cx.new(|cx| InputState::new(window, cx)),
                directory: None,
                recent: vec![],
                generation: 0,
                busy: true,
                error: None,
                tx,
                rx,
                _subscriptions: vec![],
            }
        })
    });
    visual.update(|window, cx| {
        let picker = picker.clone();
        crate::desktop::dialogs::open(window, cx, move |d, _, _| {
            d.title("选择 OMP 工作目录")
                .child(picker.clone())
                .footer(DialogFooter::new().child(DialogClose::new().trigger(|b| b.label("取消"))))
        });
    });
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let initial = visual.debug_bounds("project-directory-list").unwrap();
    let surface = visual.debug_bounds("dialog-0").unwrap();
    for error in [None, Some("读取目录失败，请检查路径".repeat(12))] {
        visual.update(|window, cx| {
            picker.update(cx, |p, cx| {
                p.busy = false;
                p.error = error;
                p.directory = Some(Directory {
                    path: HostPath::Local("fixture".into()),
                    parent: None,
                    entries: vec![DirectoryEntry {
                        name: "子目录".into(),
                        path: HostPath::Local("fixture/sub".into()),
                    }],
                });
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            visual.debug_bounds("project-directory-list").unwrap(),
            initial
        );
        assert_eq!(visual.debug_bounds("dialog-0").unwrap(), surface);
    }
}
