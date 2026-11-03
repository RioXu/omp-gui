//! Opt-in native frame benchmark. Run only against an isolated demo data directory.
use super::*;
use gpui_kit::component::WindowExt;
use gpui_kit::profiler::{self, FrameEvent, FrameTimingCollector};

impl Workspace {
    pub(super) fn start_frame_profile(window: &mut Window, cx: &mut Context<Self>) {
        let args: Vec<_> = std::env::args().collect();
        let Some(output) = args
            .windows(2)
            .find(|a| a[0] == "--frame-profile-output")
            .map(|a| std::path::PathBuf::from(&a[1]))
        else {
            return;
        };
        if !args.iter().any(|a| a == "--data-dir") {
            return;
        }
        let dialogs = args.iter().any(|a| a == "--frame-profile-dialogs");
        let exit = args.iter().any(|a| a == "--frame-profile-exit");
        cx.spawn_in(window, async move |view, cx| {
            // Wait for activation before warmup; active_switches validates the run.
            loop {
                match view.update_in(cx, |_, window, _| window.is_window_active()) {
                    Ok(true) => break,
                    Ok(false) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await
                    }
                    Err(_) => return,
                }
            }
            cx.background_executor().timer(Duration::from_secs(3)).await;
            profiler::set_trace_enabled(true);
            let mut collector = FrameTimingCollector::new();
            let mut active_switches = 0;
            for ix in 0..48 {
                if view
                    .update_in(cx, |this, window, cx| {
                        active_switches += usize::from(window.is_window_active());
                        if dialogs {
                            if ix % 2 == 0 {
                                crate::desktop::dialogs::open(window, cx, |dialog, _, _| {
                                    dialog
                                        .title("动画性能验证")
                                        .child("隔离窗口：验证弹出与关闭动画")
                                });
                            } else {
                                window.close_dialog(cx);
                            }
                        } else {
                            this.select_panel(["文件", "子智能体", "任务", "变更"][ix % 4]);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(220))
                    .await;
            }
            let events = collector.collect_unseen();
            profiler::set_trace_enabled(false);
            let samples = events
                .into_iter()
                .map(|event| match event {
                    FrameEvent::Draw(f) => serde_json::json!({
                        "type": "draw", "draw_ms": f.draw_duration().as_secs_f64()*1000.,
                        "layout_ms": f.phases.request_layout.as_secs_f64()*1000.,
                        "prepaint_ms": f.phases.prepaint.as_secs_f64()*1000.,
                        "paint_ms": f.phases.paint.as_secs_f64()*1000.,
                        "refresh_ms": f.refresh_interval.map(|v| v.as_secs_f64()*1000.)
                    }),
                    FrameEvent::Present(f) => serde_json::json!({
                        "type": "present", "submit_ms": f.present_duration().as_secs_f64()*1000.,
                        "interval_ms": f.animation_interval.map(|v| v.as_secs_f64()*1000.)
                    }),
                })
                .collect::<Vec<_>>();
            let mut intervals = samples
                .iter()
                .filter_map(|s| s["interval_ms"].as_f64())
                .collect::<Vec<_>>();
            intervals.sort_by(f64::total_cmp);
            let mean = (!intervals.is_empty())
                .then(|| intervals.iter().sum::<f64>() / intervals.len() as f64);
            let p95 = intervals.get(intervals.len() * 95 / 100).copied();
            let report = serde_json::json!({ "active_switches": active_switches,
                "backend": if cfg!(feature = "gpui-fast") {"gpui-fast"} else {"gpui-pre"},
                "switches": 48, "debug": cfg!(debug_assertions),
                "workload": if dialogs {"dialogs"} else {"tabs"},
                "summary": {"animation_intervals": intervals.len(), "mean_interval_ms": mean,
                    "p95_interval_ms": p95, "mean_fps": mean.map(|ms| 1000. / ms),
                    "foreground_valid": active_switches == 48}, "samples": samples });
            cx.background_executor()
                .spawn(async move {
                    if let Err(error) =
                        std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap())
                    {
                        eprintln!("Frame profile: {error}");
                    }
                })
                .await;
            if exit {
                let _ = view.update_in(cx, |_, _, cx| cx.quit());
            }
        })
        .detach();
    }
}
