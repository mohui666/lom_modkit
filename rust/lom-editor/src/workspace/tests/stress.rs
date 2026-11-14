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
        let story = json!({"id":"main","title":"压力测试","start":"n0","nodes":
            (0..per_chapter).map(|i| json!({"id":format!("n{i}"),"type":"say",
                "mode":"narrative","text":format!("压力测试对白 {i}，检查编辑与切换步骤是否卡顿。")
            })).collect::<Vec<_>>()});
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
    }
}
