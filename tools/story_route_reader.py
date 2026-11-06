"""Static route facts from the player's scripts; never executes or exports Lua."""
import re
from collections import deque

LEXER = re.compile(
    r"(?P<comment>--(?:\[(?P<comment_equals>=*)\[.*?\](?P=comment_equals)\]|[^\n]*))"
    r"|(?P<quoted>\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*')"
    r"|(?P<long>\[(?P<equals>=*)\[.*?\](?P=equals)\])"
    r"|(?P<word>[A-Za-z_][A-Za-z0-9_]*|\d+(?:\.\d+)?|==|~=|>=|<=|\.\.)"
    r"|(?P<symbol>\S)", re.DOTALL)
PROTECTED = re.compile(
    r"(?P<comment>--(?:\[(?P<comment_equals>=*)\[.*?\](?P=comment_equals)\]|[^\n]*))"
    r"|(?P<quoted>\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*')"
    r"|(?P<long>\[(?P<equals>=*)\[.*?\](?P=equals)\])", re.DOTALL)
CALL_LINE = re.compile(r"^\s*[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*\s*[({]")
ROUTE_CALL = re.compile(r"(?:luamanager\.(?:SetNextScript|SetCurrentStoryScript|SetTempScript|SetCurrentTravelScript)|statmodifymanager\.Mission)\s*\(")
ROUTE_SLOTS = (1, 2, "travel", "var1", "var2")


def tokens(source, routes_only=False):
    if routes_only:
        def flatten(match):
            if match.lastgroup == "comment":
                return "\n" * match.group().count("\n")
            if match.lastgroup == "long":
                width = len(match["equals"]) + 2
                value = match.group()[width:-width]
                return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('\r', '\\r').replace('\n', '\\n') + '"'
            return match.group()
        # Presentation calls dominate chapter text. They cannot change a route
        # unless they contain one of the route calls below. Keep all assignments
        # and control statements, and protect comments/strings before filtering.
        source = "\n".join(line for line in PROTECTED.sub(flatten, source).splitlines()
                           if not CALL_LINE.match(line) or ROUTE_CALL.search(line))
    result = []
    for match in LEXER.finditer(source):
        kind = match.lastgroup
        if kind == "comment":
            continue
        raw = match.group()
        if kind == "quoted":
            value = re.sub(r"\\(.)", lambda part: {"n": "\n", "r": "\r", "t": "\t"}.get(part[1], part[1]), raw[1:-1])
            result.append(("string", value))
        elif kind == "long":
            width = len(match["equals"]) + 2
            result.append(("string", raw[width:-width]))
        else:
            result.append(("word", raw))
    return result


def arguments(items, opening):
    args, current, depth = [], [], 0
    for at in range(opening + 1, len(items)):
        item = items[at]
        value = item[1] if item[0] == "word" else ""
        if value == ")" and depth == 0:
            args.append(current)
            return args
        if value == "," and depth == 0:
            args.append(current)
            current = []
        else:
            current.append(item)
            if value in ("(", "{", "["):
                depth += 1
            elif value in (")", "}", "]"):
                depth -= 1
    return []


def parse_script(name, source, text=None, positions=None, include_dynamic=False):
    text = text or {}
    items = tokens(source, routes_only=True)
    values = [item[1] for item in items]
    variables, choices, bindings, stack, assigned = {}, {}, {}, [], set()
    links, missions, unknown = [], [], 0
    dynamic, returns, link_temps, writes = {}, [], [], {}
    temps = {slot: frozenset(["@" + str(slot)]) for slot in ROUTE_SLOTS}

    def literal(arg):
        if len(arg) != 1:
            return None
        return arg[0][1] if arg[0][0] == "string" else variables.get(arg[0][1])

    def expression(parts):
        raw = " ".join(('"' + value + '"') if kind == "string" else value for kind, value in parts)
        if len(parts) == 3 and parts[1][1] == "==" and parts[2][1].isdigit():
            variable, index = parts[0][1], parts[2][1]
            if variable in bindings:
                kind, target = bindings[variable]
                if kind == "choice" and index in choices.get(target, {}):
                    key = choices[target][index]
                    return "选项：" + text.get(key, key)
                if kind == "check":
                    return "检定 " + target + "：结果 " + index
        return raw

    def add_link(target, condition):
        links.append({"target": target, "condition": condition})
        link_temps.append(temps.copy())

    at = 0
    while at < len(items):
        kind, value = items[at]
        if kind != "word":
            at += 1
            continue
        if value in ("if", "elseif"):
            end = at + 1
            while end < len(items) and items[end] != ("word", "then"):
                end += 1
            label = expression(items[at + 1:end])
            if value == "if":
                stack.append({"kind": "if", "label": label, "entry": temps.copy(),
                              "dynamic": dynamic.copy(), "arms": [], "else": False})
            elif stack and stack[-1]["kind"] == "if":
                frame = stack[-1]
                frame["arms"].append((temps.copy(), dynamic.copy()))
                temps, dynamic = frame["entry"].copy(), frame["dynamic"].copy()
                frame["label"] = "否则若 " + label
            at = end
        elif value == "else" and stack and stack[-1]["kind"] == "if":
            frame = stack[-1]
            frame["arms"].append((temps.copy(), dynamic.copy()))
            temps, dynamic = frame["entry"].copy(), frame["dynamic"].copy()
            frame["label"] = "其余分支（" + frame["label"] + "）"
            frame["else"] = True
        elif value in ("function", "do", "repeat"):
            stack.append({"kind": value, "label": "函数体内" if value == "function" else "循环或代码块内",
                          "entry": temps.copy(), "dynamic": dynamic.copy(), "arms": [], "else": False})
        elif value in ("end", "until") and stack:
            frame = stack.pop()
            arms = frame["arms"] + [(temps.copy(), dynamic.copy())]
            if not frame["else"]:
                arms.append((frame["entry"], frame["dynamic"]))
            temps = {slot: frozenset().union(*(arm[0][slot] for arm in arms)) for slot in ROUTE_SLOTS}
            dynamic = {key: result for key, result in arms[0][1].items()
                       if all(arm[1].get(key) == result for arm in arms)}
        if at + 2 < len(items) and values[at + 1] == "=":
            continues = at + 3 < len(items) and values[at + 3] in (
                "..", "+", "-", "*", "/", "%", "^", "and", "or", "==", "~=", "<", ">", "<=", ">=", "[", ".", "(")
            if items[at + 2][0] == "string" and not continues and not stack and value not in assigned:
                variables[value] = items[at + 2][1]
            else:
                variables.pop(value, None)
            assigned.add(value)
            dynamic.pop(value, None)
            if values[at + 2:at + 6] == ["checkpointmanager", ".", "Position", "("]:
                args = arguments(items, at + 5)
                if args and literal(args[0]):
                    dynamic[value] = ("position", literal(args[0]))
            elif values[at + 2:at + 6] == ["luamanager", ".", "GetTempScript", "("]:
                args = arguments(items, at + 5)
                if len(args) == 1 and len(args[0]) == 1 and args[0][0][1] in ("1", "2"):
                    dynamic[value] = ("temp", int(args[0][0][1]))
            elif values[at + 2:at + 7] == ["luamanager", ".", "GetCurrentTravelScript", "(", ")"]:
                dynamic[value] = ("temp", "travel")
            if value in ("var1", "var2"):
                binding = dynamic.get(value)
                if items[at + 2][0] == "string" and not continues:
                    temps[value] = frozenset([items[at + 2][1]])
                elif binding and binding[0] == "position":
                    temps[value] = frozenset((positions or {}).get(binding[1], [""]))
                elif binding and binding[0] == "temp":
                    temps[value] = temps[binding[1]]
                else:
                    temps[value] = frozenset([variables.get(value, "")])
                writes.setdefault(value, set()).update(target for target in temps[value] if target and not target.startswith("@"))
            if values[at + 2:at + 4] == ["choose", "("] and at + 4 < len(items):
                bindings[value] = ("choice", values[at + 4])
            elif at + 6 < len(items) and values[at + 2] == "checkpointmanager" and values[at + 4] in ("Condition", "Switch"):
                args = arguments(items, at + 5)
                if args and literal(args[0]):
                    bindings[value] = ("check", literal(args[0]))
        if at + 5 < len(items) and values[at + 1] == "[" and values[at + 3:at + 5] == ["]", "="]:
            row = items[at + 5]
            if row[0] == "string":
                choices.setdefault(value, {})[values[at + 2]] = row[1]
            elif at + 7 < len(items) and values[at + 6] == "and" and items[at + 7][0] == "string":
                choices.setdefault(value, {})[values[at + 2]] = items[at + 7][1]
        if at + 3 < len(items) and values[at + 1] == "." and values[at + 3] == "(" and (
                value == "luamanager" and values[at + 2] in ("SetNextScript", "SetCurrentStoryScript", "SetTempScript", "SetCurrentTravelScript")
                or value == "statmodifymanager" and values[at + 2] == "Mission"):
            method = values[at + 2]
            args = arguments(items, at + 3)
            condition = " / ".join(frame["label"] for frame in stack if frame["label"])
            if value == "luamanager" and method in ("SetNextScript", "SetCurrentStoryScript"):
                target = literal(args[0]) if args else None
                if target and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", target) and not target.startswith("MOD_"):
                    add_link(target, condition)
                else:
                    binding = dynamic.get(args[0][0][1]) if len(args) == 1 and len(args[0]) == 1 else None
                    variable = args[0][0][1] if len(args) == 1 and len(args[0]) == 1 else None
                    if binding and binding[0] == "position" and positions and binding[1] in positions:
                        for candidate in positions[binding[1]]:
                            add_link(candidate, " / ".join(filter(None, [condition, "场所检定 " + binding[1] + "：配置候选"])))
                    elif include_dynamic and variable in ("var1", "var2"):
                        returns.append({"slot": variable, "condition": condition, "temps": temps.copy(), "origin": binding})
                    elif binding and binding[0] == "temp" and include_dynamic:
                        returns.append({"slot": binding[1], "condition": condition, "temps": temps.copy(), "origin": binding})
                    else:
                        unknown += 1
            elif value == "luamanager" and method == "SetTempScript" and len(args) == 2:
                slot = int(args[0][0][1]) if len(args[0]) == 1 and args[0][0][1] in ("1", "2") else None
                target = literal(args[1])
                if slot:
                    temps[slot] = frozenset([target or ""])
                    if target:
                        writes.setdefault(slot, set()).add(target)
            elif value == "luamanager" and method == "SetCurrentTravelScript" and len(args) == 1:
                target = literal(args[0])
                temps["travel"] = frozenset([target or ""])
                if target:
                    writes.setdefault("travel", set()).add(target)
            elif value == "statmodifymanager" and method == "Mission" and len(args) >= 2:
                series, key = literal(args[0]), literal(args[1])
                if series and key:
                    missions.append({"series": series, "key": key, "condition": condition})
                else:
                    unknown += 1
        at += 1
    result = {"script": name, "links": links, "missions": missions, "unknown_jumps": unknown}
    if include_dynamic:
        result.update(_returns=returns, _link_temps=link_temps, _writes=writes)
    return result


def resolve_returns(routes):
    """Propagate temporary return slots through the actual chapter call graph."""
    by_name = {route["script"].lower(): route for route in routes}
    incoming = {name: {slot: set() for slot in ROUTE_SLOTS} for name in by_name}
    configured = {slot: set() for slot in ROUTE_SLOTS}
    for route in routes:
        for slot, targets in route["_writes"].items():
            configured[slot].update(target for target in targets if target.lower() in by_name)

    def state_for(name, changes):
        state = {slot: set() for slot in ROUTE_SLOTS}
        for slot, targets in changes.items():
            for target in targets:
                if target.startswith("@"):
                    reference = target[1:]
                    reference = int(reference) if reference in ("1", "2") else reference
                    state[slot].update(incoming[name][reference])
                else:
                    state[slot].add(target)
        return state

    def configured_for(item):
        candidates = set()
        for term in item["temps"][item["slot"]]:
            if term.startswith("@"):
                reference = term[1:]
                reference = int(reference) if reference in ("1", "2") else reference
                candidates.update(configured[reference])
            elif term:
                candidates.add(term)
        return candidates

    pending = deque(by_name)
    queued = set(pending)
    while pending:
        name = pending.popleft()
        queued.remove(name)
        route = by_name[name]
        outgoing = [(link["target"], state_for(name, changes))
                    for link, changes in zip(route["links"], route["_link_temps"])]
        for item in route["_returns"]:
            state = state_for(name, item["temps"])
            outgoing.extend((target, state) for target in state[item["slot"]] if target)
        for target, state in outgoing:
            target = target.lower()
            if target not in by_name:
                continue
            changed = False
            for slot in ROUTE_SLOTS:
                new = state[slot] - incoming[target][slot]
                if new:
                    incoming[target][slot].update(new)
                    changed = True
            if changed and target not in queued:
                pending.append(target)
                queued.add(target)

    for route in routes:
        name = route["script"].lower()
        for item in route.pop("_returns"):
            values = state_for(name, item["temps"])[item["slot"]]
            unbounded = "" in values
            targets = values - {""}
            detail = "调用链候选"
            if not targets and not unbounded:
                targets = configured_for(item)
                detail = "全局配置候选（调用上下文未记录）"
            if not targets or unbounded:
                route["unknown_jumps"] += 1
            for target in sorted(targets):
                origin = item["origin"]
                slot = origin[1] if origin and origin[0] == "temp" else item["slot"]
                slot_label = (f"临时章节槽 {slot}" if isinstance(slot, int)
                              else "当前游历章节" if slot == "travel" else f"跨章节变量 {slot}")
                condition = " / ".join(filter(None, [item["condition"],
                    f"返回{slot_label}：{detail}"]))
                link = {"target": target, "condition": condition}
                if link not in route["links"]:
                    route["links"].append(link)
        route.pop("_link_temps")
        route.pop("_writes")
    return routes


def read_routes(game):
    from vanilla_story_reader import resources, script_text, translations, position_routes
    environment, entries = resources(game)
    sources = {pointer.read().m_Name: script_text(pointer) for path, pointer in entries.items()
               if path.startswith("story/chinesetraditional/")}
    texts = translations(game, "\n".join(sources.values()))
    positions = position_routes(game, environment, {name.lower() for name in sources})
    routes = [parse_script(name, source, texts, positions, include_dynamic=True)
              for name, source in sorted(sources.items())]
    return {"scripts": resolve_returns(routes)}
