//! Instructions for failures outside story-field validation.

pub(super) fn os_error(message: &str, code: u32) -> bool {
    message.match_indices("os error ").any(|(at, prefix)| {
        let digits: String = message[at + prefix.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse::<u32>() == Ok(code)
    })
}

pub(crate) fn key(message: &str) -> Option<&'static str> {
    let lower = message.to_lowercase();
    let has = |patterns: &[&str]| patterns.iter().any(|pattern| lower.contains(pattern));
    let key = if has(&[
        "编辑器未退出，更新已取消",
        "无法启动 rust 更新助手",
        "更新完成，但无法重新启动编辑器",
    ]) {
        "editor.error.ops_update_manual"
    } else if has(&[
        "更新清单缺少版本",
        "无法识别发布版本",
        "此发布缺少可直接更新",
        "更新文件校验失败",
        "更新文件长度不正确",
        "拒绝下载非程序文件",
        "拒绝更新非程序文件",
    ]) {
        "editor.error.ops_update_manual"
    } else if has(&[
        "下载分片",
        "分片响应",
        "服务器未返回请求的下载分片",
        "服务器返回了未请求的分片",
        "下载地址返回了空文件",
        "下载文件超过大小限制",
        "更新文件大小超出限制",
    ]) {
        "editor.error.ops_download_invalid"
    } else if lower.contains("http status")
        && has(&["401", "403", "404", "429", "500", "502", "503", "504"])
    {
        "editor.error.ops_download_unavailable"
    } else if has(&["无法连接下载地址"]) {
        "editor.error.ops_download_unavailable"
    } else if has(&[
        "目标文件不属于当前项目",
        "目标资源不属于当前项目",
        "目标 manifest 不属于当前项目",
        "目标 scenario.json 不属于当前项目",
        "目标资源已存在",
    ]) {
        "editor.error.ops_save_conflict"
    } else if has(&[
        "多章节项目请保存为目录",
        "带事件和任务配置的作品请保存为项目目录",
    ]) {
        "editor.error.ops_save_directory"
    } else if has(&[
        "素材已被其他操作修改",
        "章节在读取期间发生变化",
        "待删除文件已被外部修改",
        "待删除章节已被外部修改",
    ]) {
        "editor.error.ops_source_changed"
    } else if has(&["保存目标不是文件", "无效保存路径", "保存目录不存在"]) {
        "editor.error.ops_save_target"
    } else if has(&[
        "符号链接",
        "目录逃逸",
        "保存路径逃逸",
        "包内不安全路径",
        "包内非规范路径",
        "不安全相对路径",
    ]) {
        "editor.error.ops_unsafe_path"
    } else if has(&[
        "package-content.sha256",
        "story-lua.sha256",
        "story/lua 完整性",
        "包缺少 manifest.json",
        "invalid zip archive",
        "zip 中央目录",
        "zip archive",
        "无法找到中央目录",
    ]) {
        "editor.error.ops_package_invalid"
    } else if has(&[
        "lua 文件需要使用 utf-8 编码",
        "旧 mod 的 utf-16 编码无效",
        "旧 mod 文件需要 utf-8 编码",
        "invalid utf-8 sequence",
        "stream did not contain valid utf-8",
    ]) {
        "editor.error.ops_text_encoding"
    } else if has(&[
        "共享库已有该 id",
        "共享库已有该内容",
        "内容 id 已被其他类型占用",
        "同时存在于多个类型",
    ]) {
        "editor.error.ops_shared_conflict"
    } else if has(&["请选择 mods 下的单个旧 mod 目录", "无法打开旧 mod 目录"]) {
        "editor.error.ops_legacy_directory"
    } else if has(&["游戏正在运行，请退出", "游戏正在运行，请先退出"]) {
        "editor.error.ops_game_busy"
    } else if has(&[
        "这不是《活侠传》游戏目录",
        "请选择包含 mortal_data",
        "游戏目录缺少图片素材包",
    ]) {
        "editor.error.ops_game_directory"
    } else if has(&[
        "bepinex 不完整",
        "bepinex 压缩包结构不完整",
        "运行时依赖缺失",
        "无法读取所选 c# 宿主文件",
        "无法读取随包 host 文件",
    ]) {
        "editor.error.ops_game_components"
    } else if has(&[
        "不支持的游戏架构",
        "游戏接入仅支持 windows",
        "不支持的 pe 架构",
    ]) {
        "editor.error.ops_game_platform"
    } else if has(&["steam 启动请求失败"]) {
        "editor.error.ops_game_launch"
    } else if has(&[
        "安装包缺少素材提取组件",
        "安装包缺少 asset-extractor",
        "无法启动素材提取组件",
        "内置组件缺失",
        "准备内置组件失败",
        "无法准备组件",
        "开发目录缺少组件",
    ]) {
        return Some("common.error.extractor_missing");
    } else if has(&[
        "素材提取组件意外退出",
        "提取组件没有返回完成结果",
        "未能提取任何游戏图片",
        "素材提取结果读取中断",
    ]) {
        "editor.error.ops_extraction_failed"
    } else if has(&["任务意外退出", "任务连接已中断", "更新下载线程意外中断"])
    {
        "editor.error.ops_background_failed"
    } else if has(&["下载已取消", "更新已取消", "提取已取消"]) {
        "editor.error.ops_cancelled"
    } else if os_error(&lower, 5) || has(&["permission denied", "拒绝访问"]) {
        "editor.error.ops_permission"
    } else if os_error(&lower, 32) || has(&["being used by another process"]) {
        "editor.error.ops_locked"
    } else if os_error(&lower, 112) || has(&["no space left", "磁盘空间不足"]) {
        "editor.error.ops_disk_full"
    } else if os_error(&lower, 2) || os_error(&lower, 3) {
        "editor.error.ops_file_missing"
    } else if os_error(&lower, 80) || os_error(&lower, 183) || lower.contains("already exists") {
        "editor.error.ops_path_occupied"
    } else {
        return None;
    };
    Some(key)
}
