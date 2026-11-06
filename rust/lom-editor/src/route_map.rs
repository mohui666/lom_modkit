use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Stroke, Vec2};
use lom_core::{story_bookmarks::Bookmark, story_routes::RouteGraph};
use std::collections::BTreeSet;

pub enum Action {
    Open(String),
    OpenMark,
}
pub struct View {
    zoom: f32,
    selected: Option<usize>,
    focus_current: bool,
    year: i32,
    month: i32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 0.85,
            selected: None,
            focus_current: true,
            year: 0,
            month: 0,
        }
    }
}
impl View {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        graph: &RouteGraph,
        marks: &[Bookmark],
        query: &str,
    ) -> Option<Action> {
        let mut action = None;
        let current: BTreeSet<_> = marks
            .first()
            .and_then(|m| graph.owners.get(&m.script.to_lowercase()))
            .into_iter()
            .flatten()
            .copied()
            .collect();
        if self.selected.is_none_or(|i| i >= graph.stations.len()) {
            self.selected = current
                .first()
                .copied()
                .or((!graph.stations.is_empty()).then_some(0));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("缩放");
            ui.add(egui::Slider::new(&mut self.zoom, 0.45..=1.4).show_value(false));
            if ui.button("回到当前标记").clicked() {
                self.selected = current.first().copied().or(self.selected);
                self.focus_current = true;
            }
            if ui
                .add_enabled(!marks.is_empty(), egui::Button::new("编辑标记对白"))
                .clicked()
            {
                action = Some(Action::OpenMark);
            }
            ui.label("年份");
            ui.add(egui::DragValue::new(&mut self.year).range(0..=9));
            ui.label("月份");
            ui.add(egui::DragValue::new(&mut self.month).range(0..=12));
            ui.small("0=全部");
        });
        ui.small("拖动空白处或滚动条移动 · 双击站点编辑 · 红色“今”为最近标记 · 金圈为后续站点 · 虚线仅表示时间顺序");
        let focus_this_frame = std::mem::take(&mut self.focus_current);
        let height = ui.available_height().min(720.0).max(300.0);
        let detail_width = (ui.available_width() * 0.27).clamp(210.0, 290.0);
        let canvas_width = (ui.available_width() - detail_width - 12.0).max(200.0);
        let next: BTreeSet<_> = graph
            .edges
            .iter()
            .filter(|e| !e.by_date && current.contains(&e.from))
            .map(|e| e.to)
            .collect();
        ui.horizontal_top(|ui|{
            ui.allocate_ui_with_layout(Vec2::new(canvas_width,height),egui::Layout::top_down(egui::Align::Min),|ui|{
                egui::ScrollArea::both().id_salt("original-route-canvas").auto_shrink([false,false]).max_height(height).show(ui,|ui|{
                    let columns=graph.stations.iter().map(|n|n.column).max().unwrap_or(0)+1;
                    let lanes=graph.stations.iter().map(|n|n.lane).max().unwrap_or(0)+1;
                    let size=Vec2::new(columns as f32*250.0+80.0,lanes as f32*155.0+120.0)*self.zoom;
                    let (canvas,_)=ui.allocate_exact_size(size,egui::Sense::hover());
                    let painter=ui.painter_at(canvas);
                    painter.rect_filled(canvas,8.0,ui.visuals().faint_bg_color);
                    let rects:Vec<_>=graph.stations.iter().map(|node|Rect::from_min_size(
                        canvas.min+Vec2::new(45.0+node.column as f32*250.0,55.0+node.lane as f32*155.0)*self.zoom,
                        Vec2::new(194.0,100.0)*self.zoom)).collect();
                    let clip=ui.clip_rect();
                    for edge in &graph.edges {
                        let from=rects[edge.from]; let to=rects[edge.to];
                        let bend=((to.left()-from.right()).abs()*0.5).max(45.0*self.zoom);
                        let extent=if edge.from==edge.to {95.0*self.zoom}else{bend};
                        if !from.union(to).expand(extent).intersects(clip){continue;}
                        let colour=if edge.by_date {Color32::from_gray(175)} else if current.contains(&edge.from){Color32::from_rgb(184,126,34)}else{Color32::from_rgb(111,133,148)};
                        let a=from.right_center(); let b=to.left_center();
                        if edge.by_date {
                            let length=a.distance(b); let count=(length/12.0).ceil().max(1.0)as usize;
                            for i in 0..count {let t=i as f32/count as f32;let end=(t+0.48/count as f32).min(1.0);painter.line_segment([a.lerp(b,t),a.lerp(b,end)],Stroke::new(self.zoom,colour));}
                        } else {
                            let controls=if edge.from==edge.to {
                                [a,a+Vec2::new(70.0,-95.0)*self.zoom,b+Vec2::new(-70.0,-95.0)*self.zoom,b]
                            }else{[a,a+Vec2::new(bend,0.0),b-Vec2::new(bend,0.0),b]};
                            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(controls,false,Color32::TRANSPARENT,Stroke::new(1.7*self.zoom,colour)));
                            painter.line_segment([b-Vec2::new(7.0,4.0)*self.zoom,b],Stroke::new(1.7*self.zoom,colour));
                            painter.line_segment([b-Vec2::new(7.0,-4.0)*self.zoom,b],Stroke::new(1.7*self.zoom,colour));
                            if from.center().y!=to.center().y {
                                let label=edge.labels.first().map(|s|s.split_once(" · ").map_or(s.as_str(),|(_,condition)|condition)).unwrap_or("");
                                let centre=a.lerp(b,0.5)+Vec2::new(0.0,-10.0*self.zoom);
                                let short=short(label,22);
                                let label_rect=painter.text(centre,Align2::CENTER_BOTTOM,short,FontId::proportional(10.5*self.zoom),colour);
                                ui.interact(label_rect,ui.id().with(("route-edge",edge.from,edge.to)),egui::Sense::hover()).on_hover_text(edge.labels.join("\n或："));
                            }
                        }
                    }
                    let mut series=BTreeSet::new();
                    for (i,node) in graph.stations.iter().enumerate() {
                        let rect=rects[i];
                        if focus_this_frame&&self.selected==Some(i) {ui.scroll_to_rect(rect,Some(egui::Align::Center));}
                        if !rect.intersects(clip.expand(120.0)){continue;}
                        if !node.entry.main&&series.insert((&node.entry.series,node.lane)) {
                            painter.text(rect.left_top()-Vec2::new(0.0,9.0*self.zoom),Align2::LEFT_BOTTOM,if node.entry.series==RouteGraph::MARKED_SERIES{"章节标记".into()}else{format!("支线 · {}",node.entry.series)},FontId::proportional(12.0*self.zoom),Color32::from_gray(120));
                        }
                        let matches=(query.is_empty()||format!("{} {} {} {} {} {}",node.entry.title,node.entry.place,node.entry.script,node.entry.series,node.entry.condition,node.scripts.join(" ")).to_lowercase().contains(query)
                            || graph.edges.iter().any(|edge|(edge.from==i||edge.to==i)&&edge.labels.iter().any(|label|label.to_lowercase().contains(query))))
                            && node.entry.matches_date(self.year,self.month,false);
                        let fill=if self.selected==Some(i){Color32::from_rgb(244,229,193)}else if node.entry.main{Color32::from_rgb(230,237,240)}else{Color32::from_rgb(235,231,239)};
                        let border=if current.contains(&i){Color32::from_rgb(185,57,52)}else if next.contains(&i){Color32::from_rgb(195,145,38)}else{Color32::from_gray(165)};
                        let title=if node.entry.title.is_empty(){&node.entry.script}else{&node.entry.title};
                        let label=format!("{}\n{}\n{} · {}",node.entry.when_label,short(title,30),node.entry.place,node.entry.script);
                        let response=ui.put(rect,egui::Button::new(RichText::new(label).size(12.5*self.zoom).color(if matches{Color32::from_gray(45)}else{Color32::from_gray(160)}))
                            .fill(if matches{fill}else{ui.visuals().faint_bg_color}).stroke(Stroke::new(if current.contains(&i)||next.contains(&i){2.2_f32}else{1.0_f32},border)).corner_radius(9.0*self.zoom));
                        let response=response.on_hover_text(format!("{}\n{}\n{}\n双击打开原版章节",node.entry.title,node.entry.key,node.entry.condition));
                        if response.clicked(){self.selected=Some(i);}
                        if response.double_clicked(){action=Some(Action::Open(node.entry.script.clone()));}
                        if current.contains(&i) {
                            let centre=rect.left_top()+Vec2::new(4.0,-2.0)*self.zoom;
                            painter.circle_filled(centre,12.0*self.zoom,Color32::from_rgb(185,57,52));
                            painter.text(centre,Align2::CENTER_CENTER,"今",FontId::proportional(13.0*self.zoom),Color32::WHITE);
                        } else if node.scripts.iter().any(|script|marks.iter().any(|mark|script.eq_ignore_ascii_case(&mark.script))) {
                            painter.text(rect.right_top()+Vec2::new(-5.0,5.0),Align2::RIGHT_TOP,"✓",FontId::proportional(14.0*self.zoom),Color32::from_rgb(52,123,99));
                        }
                    }
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(Vec2::new(detail_width,height),egui::Layout::top_down(egui::Align::Min),|ui|{
                egui::ScrollArea::vertical().id_salt("route-station-details").max_height(height).show(ui,|ui|{
                    if let Some(i)=self.selected {
                        let node=&graph.stations[i];
                        ui.heading(if node.entry.main{"主线站点"}else if node.entry.series==RouteGraph::MARKED_SERIES{"标记章节"}else{"支线站点"});
                        ui.strong(&node.entry.title); ui.label(&node.entry.when_label); ui.label(format!("地点：{}",node.entry.place));
                        ui.small(format!("{} / {}",node.entry.series,node.entry.key));
                        if !node.entry.active{ui.label("此入口需由其他剧情接入。");}
                        if !node.entry.condition.is_empty(){ui.separator();ui.strong("入口条件");ui.label(&node.entry.condition);}
                        if ui.button("编辑这个原版章节").clicked(){action=Some(Action::Open(node.entry.script.clone()));}
                        ui.separator();ui.strong("接下来");
                        for edge in graph.edges.iter().filter(|e|e.from==i&&!e.by_date) {
                            let target=&graph.stations[edge.to];
                            if ui.button(format!("{} · {}",target.entry.when_label,target.entry.title)).clicked(){self.selected=Some(edge.to);self.focus_current=true;}
                            for label in &edge.labels{ui.small(label);}
                        }
                        ui.separator();ui.strong("本段关联章节");
                        for script in &node.scripts {
                            if ui.button(script).clicked(){action=Some(Action::Open(script.clone()));}
                        }
                    }else{ui.label("选择一个站点查看时间、地点、条件与去向。");}
                    ui.separator();ui.small(format!("{} 个站点 · {} 条路线",graph.stations.len(),graph.edges.iter().filter(|e|!e.by_date).count()));
                    if graph.unknown_jumps>0 {ui.small(format!("{} 处动态去向无法静态确认，未猜测连线。",graph.unknown_jumps));}
                    ui.small("已标记的站点显示勾号；连线来自本机任务和脚本，实际触发由游戏判定。");
                });
            });
        });
        action
    }
}

fn short(value: &str, count: usize) -> String {
    let mut chars = value.chars();
    let value: String = chars.by_ref().take(count).collect();
    if chars.next().is_some() {
        format!("{value}…")
    } else {
        value
    }
}
