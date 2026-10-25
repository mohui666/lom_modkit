-- Cooperative presentation actions share the existing Host's Unity-frame clock.
-- Each branch gets one resume per parent resume, in authored order. Child errors
-- propagate to HostRunModLua's normal abort path; there is no detached Lua task.
local function __lom_parallel(actions)
    local threads = {}
    for i = 1, #actions do
        threads[i] = coroutine.create(actions[i])
    end
    while true do
        local pending = false
        for i = 1, #threads do
            local thread = threads[i]
            if coroutine.status(thread) ~= "dead" then
                local ok, message = coroutine.resume(thread)
                if not ok then
                    error("同时执行组第 " .. i .. " 个动作失败：" .. tostring(message), 0)
                end
                if coroutine.status(thread) ~= "dead" then
                    pending = true
                end
            end
        end
        if not pending then return end
        coroutine.yield()
    end
end
