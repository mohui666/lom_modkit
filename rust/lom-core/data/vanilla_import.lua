-- Keep the native speaker and dialog options across inserted editor steps.
local __lom_setcharacter = setcharacter
local __lom_setsaydialog = setsaydialog
local __lom_setmenudialog = setmenudialog
local __lom_native_context = true
local __lom_character, __lom_dialog, __lom_menu, __lom_options
local __lom_character_changed, __lom_dialog_changed, __lom_menu_changed
local function setcharacter(...)
    if __lom_native_context then
        __lom_character = {n = select("#", ...), ...}
    else
        __lom_character_changed = true
    end
    return __lom_setcharacter(...)
end
local function setsaydialog(...)
    if __lom_native_context then
        __lom_dialog = {n = select("#", ...), ...}
    else
        __lom_dialog_changed = true
    end
    return __lom_setsaydialog(...)
end
local function setmenudialog(...)
    if __lom_native_context then
        __lom_menu = {n = select("#", ...), ...}
    else
        __lom_menu_changed = true
    end
    return __lom_setmenudialog(...)
end
local function __lom_enter_custom()
    if not __lom_native_context then return end
    __lom_options = {}
    for key, value in pairs(sayoptions) do __lom_options[key] = value end
    __lom_native_context = false
end
local function __lom_restore_native()
    if __lom_native_context then return end
    for key in pairs(sayoptions) do sayoptions[key] = nil end
    for key, value in pairs(__lom_options) do sayoptions[key] = value end
    if __lom_dialog_changed and __lom_dialog then __lom_setsaydialog(table.unpack(__lom_dialog, 1, __lom_dialog.n)) end
    if __lom_character_changed and __lom_character then __lom_setcharacter(table.unpack(__lom_character, 1, __lom_character.n)) end
    if __lom_menu_changed and __lom_menu then __lom_setmenudialog(table.unpack(__lom_menu, 1, __lom_menu.n)) end
    __lom_character_changed, __lom_dialog_changed, __lom_menu_changed = false, false, false
    __lom_native_context = true
end
