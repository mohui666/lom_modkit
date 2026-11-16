use super::*;

#[cfg(windows)]
fn memory(label: &str) {
    #[repr(C)]
    struct Counters {
        size: u32,
        faults: u32,
        values: [usize; 9],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    let mut counters = Counters {
        size: std::mem::size_of::<Counters>() as u32,
        faults: 0,
        values: [0; 9],
    };
    assert_ne!(
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters,
                std::mem::size_of::<Counters>() as u32,
            )
        },
        0
    );
    println!(
        "MEMORY {label} working_mib={:.2} private_mib={:.2} peak_working_mib={:.2}",
        counters.values[1] as f64 / 1048576.0,
        counters.values[8] as f64 / 1048576.0,
        counters.values[0] as f64 / 1048576.0
    );
}

#[test]
#[cfg(windows)]
#[ignore = "manual release-build memory pressure with 100 edits and undo/redo"]
fn release_editor_history_memory_pressure() {
    let mut app = app();
    let mut project = Project::new();
    let story = json!({"id":"main","title":"压力测试","start":"n0","nodes":
        (0..2000).map(|i| json!({"id":format!("n{i}"),"type":"say","mode":"narrative",
            "text":format!("内存压力对白 {i}，检查撤销历史保留与释放。")})).collect::<Vec<_>>()});
    project.stories.insert("main".into(), story);
    for chapter in 1..10 {
        let id = format!("chapter{chapter}");
        let mut story = project.stories["main"].clone();
        story["id"] = json!(id);
        project.stories.insert(id, story);
    }
    project
        .assets
        .insert("stress-unused.bin".into(), vec![42; 64 * 1024 * 1024]);
    app.install(project);
    memory("history_open_20k_64mib");
    for edit in 0..100 {
        app.project.stories.get_mut("main").unwrap()["nodes"][1999]["text"] =
            json!(format!("内存编辑 {edit}"));
        app.track();
        app.flush();
    }
    assert_eq!(app.undo.len(), 100);
    memory("history_after_100_edits");
    for _ in 0..50 {
        app.undo();
    }
    assert_eq!(
        app.project.stories["main"]["nodes"][1999]["text"],
        "内存编辑 49"
    );
    memory("history_after_50_undo");
    for _ in 0..50 {
        app.redo();
    }
    assert_eq!(
        app.project.stories["main"]["nodes"][1999]["text"],
        "内存编辑 99"
    );
    memory("history_after_50_redo");
    for id in 1..10 {
        assert_eq!(
            app.project.stories[&format!("chapter{id}")]["nodes"][1999]["text"],
            "内存压力对白 1999，检查撤销历史保留与释放。"
        );
    }
    app.install(Project::new());
    memory("history_after_close");
}

#[test]
#[ignore = "manual release-build stress profile of editor frame and edit latency"]
fn release_editor_stress_profile() {
    fn report(label: &str, values: &mut [f64]) {
        values.sort_by(f64::total_cmp);
        println!(
            "STRESS {label} p50_ms={:.3} p95_ms={:.3} max_ms={:.3}",
            values[values.len() / 2],
            values[values.len() * 95 / 100],
            values[values.len() - 1]
        );
    }
    let cases = std::env::var("LOM_STRESS_NODES")
        .ok()
        .map(|value| vec![value.parse::<usize>().unwrap()])
        .unwrap_or_else(|| vec![2000, 10000]);
    for per_chapter in cases {
        let mut app = app();
        let mut project = Project::new();
        let mut story = json!({"story_schema":2,"id":"main","title":"压力测试","start":"n0","nodes":
            (0..per_chapter).map(|i| json!({"id":format!("n{i}"),"type":"say",
                "mode":"narrative","text":format!("压力测试对白 {i}，检查编辑与切换步骤是否卡顿。")
            })).collect::<Vec<_>>()});
        if std::env::var_os("LOM_STRESS_GROUPED").is_some() {
            story["_editor"] = json!({"sections":(0..per_chapter).step_by(200).map(|start| {
                let end = (start + 199).min(per_chapter - 1);
                json!({"id":format!("section{start}"),"title":"分区","start":format!("n{start}"),"end":format!("n{end}"),
                    "groups":(start..=end).step_by(50).map(|a| json!({"id":format!("group{a}"),"title":"分组","start":format!("n{a}"),"end":format!("n{}",(a+49).min(end))})).collect::<Vec<_>>()})
            }).collect::<Vec<_>>()});
        }
        project.stories.insert("main".into(), story);
        for chapter in 1..10 {
            let id = format!("chapter{chapter}");
            let mut story = project.stories["main"].clone();
            story["id"] = json!(id);
            project.stories.insert(id, story);
        }
        for asset in 0..4 {
            project.assets.insert(
                format!("assets/stress-unused-{asset}.bin"),
                vec![42; 16 * 1024 * 1024],
            );
        }
        app.install(project);
        println!("STRESS project chapters=10 nodes_per_chapter={per_chapter} total_nodes={} media_mib=64 debug={}", per_chapter * 10, cfg!(debug_assertions));
        let ctx = egui::Context::default();
        let draw = |app: &mut App, mode: usize| {
            let mut parts = [0.0; 4];
            std::hint::black_box(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    for (index, action) in [App::sidebar, App::right, App::center]
                        .into_iter()
                        .enumerate()
                    {
                        let start = Instant::now();
                        if mode == 3 || mode == index {
                            action(app, ctx);
                        }
                        parts[index] = start.elapsed().as_secs_f64() * 1000.0;
                    }
                },
            ));
            let start = Instant::now();
            app.track();
            parts[3] = start.elapsed().as_secs_f64() * 1000.0;
            parts
        };
        for (view, label) in [(View::Stage, "stage"), (View::Graph, "graph")] {
            app.view = view;
            app.selected = 0;
            for _ in 0..3 {
                draw(&mut app, 3);
            }
            let mut parts = [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            for _ in 0..40 {
                let start = Instant::now();
                let times = draw(&mut app, 3);
                for i in 0..4 {
                    parts[i].push(times[i]);
                }
                parts[4].push(start.elapsed().as_secs_f64() * 1000.0);
            }
            for (i, part) in ["sidebar", "preview", "form", "track", "frame"]
                .into_iter()
                .enumerate()
            {
                report(&format!("{per_chapter}_{label}_{part}"), &mut parts[i]);
            }
        }
        app.view = View::Stage;
        for selected in [per_chapter / 2, per_chapter - 1] {
            app.selected = selected;
            let start = Instant::now();
            draw(&mut app, 3);
            println!(
                "STRESS {per_chapter}_select_{selected} cold_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
        let mut edit = Vec::new();
        for i in 0..10 {
            let start = Instant::now();
            app.project.stories.get_mut("main").unwrap()["nodes"][per_chapter - 1]["text"] =
                json!(format!("编辑压力检查 {i}"));
            app.track();
            edit.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        report(&format!("{per_chapter}_edit_capture"), &mut edit);
        // Include the frame after each keystroke: idle frames reuse the preview,
        // whereas typing used to discard every prefix and replay the chapter.
        for parallel in [false, true] {
            if parallel {
                app.project.stories.get_mut("main").unwrap()["nodes"][per_chapter - 1] = json!({
                    "id":format!("n{}", per_chapter - 1),"type":"parallel","actions":[
                        {"type":"show","character":"player","position":"L","portrait":"normal"},
                        {"type":"offset","character":"player","x":25,"y":10,"duration":0.5},
                        {"type":"wait","seconds":1}
                    ]
                });
                app.track();
            }
            let mut editing = [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            for i in 0..20 {
                let start = Instant::now();
                let node =
                    &mut app.project.stories.get_mut("main").unwrap()["nodes"][per_chapter - 1];
                if parallel {
                    node["actions"][1]["x"] = json!(i);
                } else {
                    node["text"] = json!(format!("连续打字检查 {i}"));
                }
                app.track();
                let times = draw(&mut app, 3);
                for part in 0..4 {
                    editing[part].push(times[part]);
                }
                editing[4].push(start.elapsed().as_secs_f64() * 1000.0);
            }
            let kind = if parallel { "parallel_edit" } else { "typing" };
            for (i, part) in ["sidebar", "preview", "form", "track", "frame"]
                .into_iter()
                .enumerate()
            {
                report(&format!("{per_chapter}_{kind}_{part}"), &mut editing[i]);
            }
        }
        let saved = tempfile::tempdir().unwrap();
        let start = Instant::now();
        assert!(app.save_at(saved.path(), false), "{:?}", app.error);
        let request_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut saving_frames = Vec::new();
        while app.saving() {
            let frame = Instant::now();
            draw(&mut app, 3);
            saving_frames.push(frame.elapsed().as_secs_f64() * 1000.0);
            app.poll_background_save(&ctx);
            assert!(start.elapsed() < Duration::from_secs(30), "保存未完成");
        }
        println!(
            "STRESS {per_chapter}_background_save request_ms={request_ms:.3} total_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0
        );
        if !saving_frames.is_empty() {
            report(&format!("{per_chapter}_saving_frame"), &mut saving_frames);
        }
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(!app.dirty());
        let reopened = Project::open(saved.path()).unwrap();
        assert_eq!(reopened.stories, app.project.stories);
        assert_eq!(reopened.assets.len(), app.project.assets.len());
    }
}
